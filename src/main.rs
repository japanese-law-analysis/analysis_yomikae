use analysis_yomikae::*;
use anyhow::Result;
use clap::Parser;
use jplaw_text::{xml_to_law_text, LawContents};
use std::path::Path;
use tokio::{
  self,
  fs::*,
  io::{AsyncReadExt, AsyncWriteExt},
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
  let law_data_lst = listup_law::get_law_from_index(&args.index_file).await?;
  info!("[END] get law data: {:?}", &args.index_file);
  let mut law_data_stream = tokio_stream::iter(law_data_lst);

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
  while let Some(law_data) = law_data_stream.next().await {
    let num = law_data.num;
    let file_name = law_data.file;
    let file_path = work_dir_path.join(file_name);
    info!("[START] work file: {:?}", file_path);
    let mut f = File::open(&file_path).await?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).await?;
    let law_text_lst = xml_to_law_text(&buf).await?;
    let yomikae_law_text_lst = law_text_lst
      .iter()
      .filter(|lawtext| match &lawtext.contents {
        LawContents::Text(s) => s.contains("と読み替える。"),
        _ => false,
      })
      .collect::<Vec<_>>();
    let mut yomikae_law_text_stream = tokio_stream::iter(yomikae_law_text_lst);
    while let Some(law_text) = yomikae_law_text_stream.next().await {
      let yomikae_info_lst_res =
        analysis_yomikae::parse_yomikae(law_text, &num, &law_text.article_info).await;
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
              article: law_text.article_info.clone(),
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

  output_file.write_all("\n]".as_bytes()).await?;
  info!("[END write json file");
  output_file.flush().await?;

  error_output_file.write_all("\n]".as_bytes()).await?;
  info!("[END write error output file");
  error_output_file.flush().await?;

  Ok(())
}
