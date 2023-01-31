use analysis_yomikae::*;
use anyhow::{Context, Result};
use clap::Parser;
use jplaw_text::ArticleTargetInfo;
use quick_xml::Reader;
use search_article_with_word::{self, Chapter};
use std::collections::HashMap;
use std::path::Path;
use tokio::{
  self,
  fs::*,
  io::{AsyncWriteExt, BufReader},
};
use tokio_stream::StreamExt;
use tracing::*;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
  /// 解析結果を出力するJSONファイルへのpath
  #[clap(short, long)]
  output: String,
  /// エラーが出た条文の情報を出力するJSONファイルへのpath
  #[clap(short, long)]
  error_output: String,
  /// 法令XMLファイル群が置かれている作業ディレクトリへのpath
  #[clap(short, long)]
  work: String,
  /// 法令ファイルのインデックス情報が書かれたJSONファイルへのpath
  #[clap(short, long)]
  index_file: String,
  /// 解析する対象の条文のインデックスが書かれたJSONファイルへのpath
  #[clap(short, long)]
  article_info_file: String,
}

async fn init_logger() -> Result<()> {
  let subscriber = tracing_subscriber::fmt()
    .with_max_level(tracing::Level::INFO)
    .finish();
  tracing::subscriber::set_global_default(subscriber)?;
  Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
  let args = Args::parse();

  init_logger().await?;

  info!("[START] get law data: {:?}", &args.index_file);
  let raw_data_lst = listup_law::get_law_from_index(&args.index_file).await?;
  info!("[END] get law data: {:?}", &args.index_file);
  let mut raw_data_stream = tokio_stream::iter(raw_data_lst);
  let mut file_index = HashMap::new();
  while let Some(raw_data) = raw_data_stream.next().await {
    let num = raw_data.num;
    file_index.insert(num, raw_data.file);
  }

  info!("[START] get article info: {:?}", &args.article_info_file);
  let raw_paragraph_lst =
    search_article_with_word::get_law_from_artcile_info(&args.article_info_file).await?;
  info!("[END] get article info: {:?}", &args.article_info_file);

  let mut law_paragraph_stream = tokio_stream::iter(raw_paragraph_lst);

  let work_dir_path = Path::new(&args.work);

  let mut error_lst = Vec::new();
  let mut error_output_file = File::create(&args.error_output).await?;
  info!("[START] write error output file");
  error_output_file.write_all("[".as_bytes()).await?;

  let mut output_file = File::create(&args.output).await?;
  info!("[START] write json file");
  output_file.write_all("[".as_bytes()).await?;

  let mut is_head = true;
  let mut is_error_head = true;
  while let Some(law_paragraph) = law_paragraph_stream.next().await {
    let num = law_paragraph.num;
    let file_name = file_index
      .get(&num)
      .with_context(|| format!("not found file name with law num: {num}"))?;
    let file_path = work_dir_path.join(file_name);
    info!("[START] work file: {:?}", file_path);
    let chapter_lst = law_paragraph.chapter_data;
    let mut chapter_stream = tokio_stream::iter(chapter_lst);
    while let Some(chapter) = chapter_stream.next().await {
      info!("[DATA] chapter; {num}:{chapter:?}");
      let mut reader = Reader::from_reader(BufReader::new(File::open(&file_path).await?));
      let target = target_info_from_chapter_lst(&chapter).await;
      let law_text_lst = jplaw_text::search_law_text(&mut reader, &target).await?;
      let mut law_text_stream = tokio_stream::iter(law_text_lst).filter(|c| !c.is_child);
      while let Some(law_text) = law_text_stream.next().await {
        let yomikae_info_lst_res = analysis_yomikae::parse_yomikae(&law_text, &num, &chapter).await;
        match yomikae_info_lst_res {
          Ok(yomikae_info_lst) => {
            if !yomikae_info_lst.is_empty() {
              let mut yomikae_info_stream = tokio_stream::iter(yomikae_info_lst);
              while let Some(yomikae_info) = yomikae_info_stream.next().await {
                let yomikae_info_json_str = serde_json::to_string(&yomikae_info)?;
                if is_head {
                  output_file.write_all("\n".as_bytes()).await?;
                  is_head = false;
                } else {
                  output_file.write_all(",\n".as_bytes()).await?;
                };
                output_file
                  .write_all(yomikae_info_json_str.as_bytes())
                  .await?;
              }
            } else {
              let law_info = LawInfo {
                num: num.to_string(),
                chapter: chapter.clone(),
                contents: law_text.clone(),
              };
              let err = YomikaeError::NotFoundYomikae(law_info);
              let mut error_stream = tokio_stream::iter(&error_lst);
              let is_err_exist = error_stream.any(|e| e == &err).await;
              if !is_err_exist {
                error_lst.push(err.clone());
                if is_error_head {
                  error_output_file.write_all("\n".as_bytes()).await?;
                  is_error_head = false;
                } else {
                  error_output_file.write_all(",\n".as_bytes()).await?;
                };
                error_output_file
                  .write_all(serde_json::to_string(&err)?.as_bytes())
                  .await?;
              };
            }
          }
          Err(err) => {
            error!("{err}");
            let mut error_stream = tokio_stream::iter(&error_lst);
            let is_err_exist = error_stream.any(|e| e == &err).await;
            if !is_err_exist {
              error_lst.push(err.clone());
              if is_error_head {
                error_output_file.write_all("\n".as_bytes()).await?;
                is_error_head = false;
              } else {
                error_output_file.write_all(",\n".as_bytes()).await?;
              };
              error_output_file
                .write_all(serde_json::to_string(&err)?.as_bytes())
                .await?;
            };
          }
        }
      }
    }
  }

  output_file.write_all("\n]".as_bytes()).await?;
  info!("[END write json file");
  output_file.flush().await?;

  error_output_file.write_all("\n]".as_bytes()).await?;
  info!("[END write error output file");
  error_output_file.flush().await?;

  Ok(())
}

async fn target_info_from_chapter_lst(chapter: &Chapter) -> ArticleTargetInfo {
  ArticleTargetInfo {
    article: chapter.article.clone(),
    paragraph: chapter.paragraph.clone(),
    item: chapter.item.clone(),
    sub_item: chapter.sub_item.clone(),
    suppl_provision_title: chapter.suppl_provision_title.clone(),
  }
}
