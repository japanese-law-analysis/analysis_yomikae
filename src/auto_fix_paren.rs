use tracing::*;

/// カギカッコの種類
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Paren {
  Open,
  Close,
}

/// カギカッコの種類と位置情報を保持する
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParenInfo {
  pos: usize,
  v: Paren,
}

/// 分割位置の候補の情報。
/// 「何文字で何回分割するのか」を保持する。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitPattern {
  /// 何文字で分割するか
  len: usize,
  /// 何回使うか
  times: usize,
}

/// 分割位置の候補の情報。
/// 「何文字で分割するのか」を保持する。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitPatternList {
  /// 現在採用している候補
  now: usize,
  /// 何文字で分割するのか、の候補
  pattern_lst: Vec<SplitPattern>,
}

/// 改め文や読み替え規定文に出現するカギカッコ付きの文章を、
/// 開きカギカッコと閉じカギカッコの非対応があっても分割する関数
pub async fn auto_fix_paren(text: &str) -> Option<Vec<String>> {
  let mut paren_info_lst = Vec::new();
  let mut maru_paren_depth = 0;
  let mut paren_in_text_tmp = Vec::new();
  for (i, c) in text.chars().enumerate() {
    match c {
      '「' => {
        if maru_paren_depth == 0 {
          paren_info_lst.push(ParenInfo {
            pos: i,
            v: Paren::Open,
          })
        } else {
          paren_in_text_tmp.push(ParenInfo {
            pos: i,
            v: Paren::Open,
          })
        }
      }
      '」' => {
        if maru_paren_depth == 0 {
          paren_info_lst.push(ParenInfo {
            pos: i,
            v: Paren::Close,
          })
        } else {
          paren_in_text_tmp.push(ParenInfo {
            pos: i,
            v: Paren::Close,
          })
        }
      }
      '（' => maru_paren_depth += 1,
      '）' => {
        if maru_paren_depth == 1 {
          paren_in_text_tmp = Vec::new();
          maru_paren_depth = 0;
        } else {
          maru_paren_depth -= 1;
        }
      }
      _ => (),
    }
  }
  if !paren_in_text_tmp.is_empty() {
    info!("[WARNING] paren_in_text_tmp is not none");
    paren_info_lst.append(&mut paren_in_text_tmp);
  }
  println!("paren_info_lst: {paren_info_lst:?}");

  let mut now_head: usize = 0;
  let mut pattern: Vec<SplitPatternList> = Vec::new();
  while now_head < paren_info_lst.len() {
    // 最短の取得を許される最大の回数から取っていくようにする
    // 典型的なパターンでは最速で終わり、込み入った例外パターンではより多様な選択を検証できる
    let len_max = (paren_info_lst.len() - now_head) / 2;
    let mut pattern_lst = Vec::new();
    for len in 2..=len_max {
      let mut max_times = None;
      for times in (2..=((paren_info_lst.len() - now_head) / len)).rev() {
        if max_times.is_none() {
          let paren_lst_lst = (1..=times)
            .map(|n| {
              let pos_start = now_head + len * (n - 1);
              let pos_end = now_head + len * n - 1;
              let v = &paren_info_lst[pos_start..=pos_end]
                .iter()
                .map(|info| info.clone().v)
                .collect::<Vec<_>>();
              v.clone()
            })
            .collect::<Vec<_>>();
          let head = &paren_lst_lst[0];
          if paren_lst_lst.iter().all(|paren_lst| {
            paren_lst == head
              && paren_lst[0] == Paren::Open
              && paren_lst[paren_lst.len() - 1] == Paren::Close
          }) {
            max_times = Some(times);
          }
        }
      }
      if let Some(max_times) = max_times {
        let mut l = (2..=max_times)
          .rev()
          .map(|times| SplitPattern { len, times })
          .collect::<Vec<_>>();
        pattern_lst.append(&mut l);
      }
    }

    if pattern_lst.is_empty() {
      // 次の分割候補がないのでトラックバックする
      if let Some(split_pattern) = pattern.pop() {
        if split_pattern.now < split_pattern.pattern_lst.len() - 1 {
          now_head -= split_pattern.pattern_lst[split_pattern.now].len
            * split_pattern.pattern_lst[split_pattern.now].times;
          now_head += split_pattern.pattern_lst[split_pattern.now + 1].len
            * split_pattern.pattern_lst[split_pattern.now + 1].times;
          pattern.push(SplitPatternList {
            now: split_pattern.now + 1,
            pattern_lst: split_pattern.pattern_lst,
          })
        } else {
          now_head -= split_pattern.pattern_lst[split_pattern.now].len
            * split_pattern.pattern_lst[split_pattern.now].times;
        }
      } else {
        // 分割位置が定まらないためその旨を返す
        return None;
      }
    } else {
      // 分割できたので加える
      let len = pattern_lst[0].len;
      let n = pattern_lst[0].times;
      now_head += len * n;
      pattern.push(SplitPatternList {
        now: 0,
        pattern_lst,
      });
    }
  }

  let mut v = Vec::new();
  let mut paren_pos = 0;
  let mut char_pos = 0;
  let chars = text.chars().collect::<Vec<_>>();
  for SplitPatternList { now, pattern_lst } in pattern.iter() {
    let len = pattern_lst[*now].len;
    let times = pattern_lst[*now].times;
    for n in 1..=times {
      let start = paren_info_lst[paren_pos + len * (n - 1)].pos;
      let end = paren_info_lst[paren_pos + (len * n) - 1].pos;
      let s1 = &chars[char_pos..start].iter().collect::<String>();
      let s2 = &chars[start..=end].iter().collect::<String>();
      char_pos = end + 1;
      v.push(s1.clone());
      v.push(s2.clone());
    }
    paren_pos += len * times;
  }
  let s = &chars[char_pos..].iter().collect::<String>();
  v.push(s.clone());
  Some(v)
}

#[tokio::test]
async fn check_auto_fix_paren1() {
  assert_eq!(
    auto_fix_paren("あ「い」う「え」お").await.unwrap(),
    vec![
      "あ".to_string(),
      "「い」".to_string(),
      "う".to_string(),
      "「え」".to_string(),
      "お".to_string()
    ]
  );
}

#[tokio::test]
async fn check_auto_fix_paren2() {
  assert_eq!(
    auto_fix_paren("あ「い」」う「え」」お").await.unwrap(),
    vec![
      "あ".to_string(),
      "「い」」".to_string(),
      "う".to_string(),
      "「え」」".to_string(),
      "お".to_string()
    ]
  );
}

#[tokio::test]
async fn check_auto_fix_paren3() {
  assert_eq!(
    auto_fix_paren("あ「「い」う「「え」お").await.unwrap(),
    vec![
      "あ".to_string(),
      "「「い」".to_string(),
      "う".to_string(),
      "「「え」".to_string(),
      "お".to_string()
    ]
  );
}

#[tokio::test]
async fn check_auto_fix_paren4() {
  assert_eq!(
    auto_fix_paren("あ「い」う」え」お「か」き」く」け")
      .await
      .unwrap(),
    vec![
      "あ".to_string(),
      "「い」う」え」".to_string(),
      "お".to_string(),
      "「か」き」く」".to_string(),
      "け".to_string()
    ]
  );
}

#[tokio::test]
async fn check_auto_fix_paren5() {
  assert_eq!(
    auto_fix_paren("あ「た」ち「つ」て」と「な」に「ぬ」ね」の")
      .await
      .unwrap(),
    vec![
      "あ".to_string(),
      "「た」ち「つ」て」".to_string(),
      "と".to_string(),
      "「な」に「ぬ」ね」".to_string(),
      "の".to_string()
    ]
  );
}

#[tokio::test]
async fn check_auto_fix_paren6() {
  assert_eq!(
    auto_fix_paren("あ「い」」う「え」」お「か「き」く」け」こ「さ「し」す」せ」そ「た」ち「つ」て」と「な」に「ぬ」ね」の")
      .await
      .unwrap(),
    vec![
      "あ".to_string(),
      "「い」」".to_string(),
      "う".to_string(),
      "「え」」".to_string(),
      "お".to_string(),
      "「か「き」く」け」".to_string(),
      "こ".to_string(),
      "「さ「し」す」せ」".to_string(),
      "そ".to_string(),
      "「た」ち「つ」て」".to_string(),
      "と".to_string(),
      "「な」に「ぬ」ね」".to_string(),
      "の".to_string(),
    ]
  );
}

#[tokio::test]
async fn check_auto_fix_paren7() {
  assert_eq!(
    auto_fix_paren("あ「い」」う「え」」お「か」」き「く」け「こ」さ")
      .await
      .unwrap(),
    vec![
      "あ".to_string(),
      "「い」」".to_string(),
      "う".to_string(),
      "「え」」".to_string(),
      "お".to_string(),
      "「か」」".to_string(),
      "き".to_string(),
      "「く」".to_string(),
      "け".to_string(),
      "「こ」".to_string(),
      "さ".to_string()
    ]
  );
}

#[tokio::test]
async fn check_auto_fix_paren8() {
  assert_eq!(
    auto_fix_paren("あ「い」」う「え」」お「か」」き")
      .await
      .unwrap(),
    vec![
      "あ".to_string(),
      "「い」」".to_string(),
      "う".to_string(),
      "「え」」".to_string(),
      "お".to_string(),
      "「か」」".to_string(),
      "き".to_string()
    ]
  );
}

#[tokio::test]
async fn check_auto_fix_paren9() {
  assert_eq!(
    auto_fix_paren("あ「い」う「え（「お」）か」き")
      .await
      .unwrap(),
    vec![
      "あ".to_string(),
      "「い」".to_string(),
      "う".to_string(),
      "「え（「お」）か」".to_string(),
      "き".to_string()
    ]
  );
}

#[tokio::test]
async fn check_auto_fix_paren10() {
  assert_eq!(
    auto_fix_paren("あ「い」う「え（」お「か（」き")
      .await
      .unwrap(),
    vec![
      "あ".to_string(),
      "「い」".to_string(),
      "う".to_string(),
      "「え（」".to_string(),
      "お".to_string(),
      "「か（」".to_string(),
      "き".to_string()
    ]
  );
}

//<https://elaws.e-gov.go.jp/document?lawid=129AC0000000089#501AC0000000071-Sp-Pr_1-It_3>
//第一条中外国法人の登記及び夫婦財産契約の登記に関する法律第四条の改正規定（「並びに第百三十二条」を「、第百三十二条から第百三十七条まで並びに第百三十九条」に改める部分に限る。）、第三条から第五条までの規定、第六条中商業登記法第七条の二、第十一条の二、第十五条、第十七条及び第十八条の改正規定、同法第四十八条の前の見出しを削る改正規定、同条から同法第五十条まで並びに同法第八十二条第二項及び第三項の改正規定、同条第四項の改正規定（「本店の所在地における」を削る部分に限る。）、同法第八十七条第一項及び第二項並びに第九十一条第一項の改正規定、同条第二項の改正規定（「本店の所在地における」を削る部分に限る。）並びに同法第九十五条、第百十一条、第百十八条及び第百三十八条の改正規定、第九条中社債、株式等の振替に関する法律第百五十一条第二項第一号の改正規定、同法第百五十五条第一項の改正規定（「（以下この条」の下に「及び第百五十九条の二第二項第四号」を加える部分に限る。）、同法第百五十九条の次に一条を加える改正規定、同法第二百二十八条第二項の表第百五十九条第三項第一号の項の次に次のように加える改正規定、同法第二百三十五条第一項の改正規定（「まで」の下に「、第百五十九条の二第二項第四号」を加える部分に限る。）、同条第二項の表第百五十九条第一項の項の次に次のように加える改正規定及び同法第二百三十九条第二項の表に次のように加える改正規定、第十条第二項から第二十三項までの規定、第十一条中会社更生法第二百六十一条第一項後段を削る改正規定、第十四条中会社法の施行に伴う関係法律の整備等に関する法律第四十六条の改正規定、第十五条中一般社団法人及び一般財団法人に関する法律の目次の改正規定（「従たる事務所の所在地における登記（第三百十二条―第三百十四条）」を「削除」に改める部分に限る。）、同法第四十七条の次に五条を加える改正規定、同法第三百一条第二項第四号の次に一号を加える改正規定、同法第六章第四節第三款、第三百十五条及び第三百二十九条の改正規定、同法第三百三十条の改正規定（「第四十九条から第五十二条まで」を「第五十一条、第五十二条」に、「及び第百三十二条」を「、第百三十二条から第百三十七条まで及び第百三十九条」に改め、「、「支店」とあるのは「従たる事務所」と」を削る部分に限る。）並びに同法第三百四十二条第十号の次に一号を加える改正規定、第十七条中信託法第二百四十七条の改正規定（「（第三項を除く。）、第十八条」を削る部分に限る。）、第十八条の規定（前号に掲げる改正規定を除く。）、第二十二条及び第二十三条の規定、第二十五条中金融商品取引法第八十九条の三の改正規定、同法第八十九条の四第二項を削る改正規定、同法第九十条の改正規定（「第十七条から」の下に「第十九条の三まで、第二十一条から」を加え、「第十五号及び第十六号」を「第十四号及び第十五号」に改める部分、「及び第二十条第三項」を削る部分及び「読み替える」を「、同法第百四十六条の二中「商業登記法（」とあるのは「金融商品取引法（昭和二十三年法律第二十五号）第九十条において準用する商業登記法（」と、「商業登記法第百四十五条」とあるのは「金融商品取引法第九十条において準用する商業登記法第百四十五条」と読み替える」に改める部分を除く。）、同法第百条の四、第百一条の二十第一項、第百二条第一項及び第百二条の十の改正規定、同法第百二条の十一の改正規定（「第十七条から」の下に「第十九条の三まで、第二十一条から」を加え、「第十五号及び第十六号」を「第十四号及び第十五号」に改める部分、「及び第二十条第三項」を削る部分及び「読み替える」を「、同法第百四十六条の二中「商業登記法（」とあるのは「金融商品取引法（昭和二十三年法律第二十五号）第百二条の十一において準用する商業登記法（」と、「商業登記法第百四十五条」とあるのは「金融商品取引法第百二条の十一において準用する商業登記法第百四十五条」と読み替える」に改める部分を除く。）並びに同法第百四十五条第一項及び第百四十六条の改正規定、第二十七条中損害保険料率算出団体に関する法律第二十三条から第二十四条の二までの改正規定及び同法第二十五条の改正規定（「第二十三条の二まで、」を「第十九条の三まで（登記申請の方式、申請書の添付書面、申請書に添付すべき電磁的記録、添付書面の特例）、第二十一条から」に、「第十五号及び第十六号」を「第十四号」に改める部分を除く。）、第三十二条中投資信託及び投資法人に関する法律第九十四条第一項の改正規定（「第三百五条第一項本文及び第四項」の下に「から第六項まで」を加える部分を除く。）、同法第百六十四条第四項の改正規定、同法第百六十六条第二項第八号の次に一号を加える改正規定、同法第百七十七条の改正規定（「、第二十条第一項及び第二項」を削る部分及び「、同法第二十四条第七号中「若しくは第三十条第二項若しくは」とあるのは「若しくは」と」を削り、「第百七十五条」と」の下に「、同法第百四十六条の二中「商業登記法（」とあるのは「投資信託及び投資法人に関する法律（昭和二十六年法律第百九十八号）第百七十七条において準用する商業登記法（」と、「商業登記法第百四十五条」とあるのは「投資信託及び投資法人に関する法律第百七十七条において準用する商業登記法第百四十五条」と」を加える部分を除く。）及び同法第二百四十九条第十九号の次に一号を加える改正規定、第三十四条中信用金庫法の目次の改正規定（「第四十八条の八」を「第四十八条の十三」に改める部分に限る。）、同法第四十六条第一項の改正規定、同法第四章第七節中第四十八条の八の次に五条を加える改正規定、同法第六十五条第二項、第七十四条から第七十六条まで及び第七十七条第四項の改正規定、同法第八十五条の改正規定（前号に掲げる部分を除く。）、同法第八十七条の四第四項の改正規定並びに同法第九十一条第一項第十二号の次に一号を加える改正規定、第三十六条中労働金庫法第七十八条から第八十条まで及び第八十一条第四項の改正規定並びに同法第八十九条の改正規定（前号に掲げる部分を除く。）、第三十八条中金融機関の合併及び転換に関する法律第六十四条第一項の改正規定、第四十条の規定（同条中協同組織金融機関の優先出資に関する法律第十四条第二項及び第二十二条第五項第三号の改正規定を除く。）、第四十一条中保険業法第四十一条第一項の改正規定、同法第四十九条第一項の改正規定（「規定中」を「規定（同法第二百九十八条（第一項第三号及び第四号を除く。）、第三百十一条第四項並びに第五項第一号及び第二号、第三百十二条第五項並びに第六項第一号及び第二号、第三百十四条、第三百十八条第四項、第三百二十五条の二並びに第三百二十五条の五第二項を除く。）中「株主」とあるのは「総代」と、これらの規定（同法第二百九十九条第一項及び第三百二十五条の三第一項第五号を除く。）中」に改め、「とあり、及び「取締役会設置会社」」を削り、「相互会社」と、」の下に「これらの規定中」を加え、「、これらの規定（同法第二百九十八条第一項（各号を除く。）及び第四項、第三百十一条第四項、第三百十二条第五項、第三百十四条並びに第三百十八条第四項を除く。）中「株主」とあるのは「総代」と」を削り、「各号を除く。）及び第四項中」を「第三号及び第四号を除く。）中「前条第四項」とあるのは「保険業法第四十五条第二項」と、「株主」とあるのは「社員又は総代」と、「次項本文及び次条から第三百二条まで」とあるのは「次条及び第三百条」と、同条第四項中「取締役会設置会社」とあるのは「相互会社」と、」に、「第三百十一条第四項及び第三百十二条第五項」を「第三百十一条第一項中「議決権行使書面に」とあるのは「議決権行使書面（保険業法第四十八条第三項に規定する議決権行使書面をいう。以下同じ。）に」と、同条第四項並びに第五項第一号及び第二号並びに同法第三百十二条第五項並びに第六項第一号及び第二号」に改め、「共同」を削る部分を除く。）、同法第六十四条第二項及び第三項の改正規定、同法第六十七条の改正規定（「、第四十八条」を「、第五十一条」に改め、「支店所在地における登記、」を削り、「登記）並びに」を「登記）、」に、「第百四十八条」を「第百三十七条」に、「職権抹消、」を「職権抹消）並びに第百三十九条から第百四十八条まで（」に改める部分及び「第四十八条から第五十三条までの規定中「本店」とあるのは「主たる事務所」と、「支店」とあるのは「従たる事務所」を「第四十七条第三項中「前項」とあるのは「保険業法第六十四条第一項」と、同法第五十五条第一項中「会社法第三百四十六条第四項」とあるのは「保険業法第五十三条の十二第四項」と、同法第百四十六条の二中「商業登記法（」とあるのは「保険業法（平成七年法律第百五号）第六十七条において準用する商業登記法（」と、「商業登記法第百四十五条」とあるのは「保険業法第六十七条において準用する商業登記法第百四十五条」と、同法第百四十八条中「この法律に」とあるのは「保険業法に」と、「この法律の施行」とあるのは「相互会社に関する登記」に改める部分に限る。）、同法第八十四条第一項並びに第九十六条の十四第一項及び第二項の改正規定、同法第九十六条の十六第四項の改正規定（「並びに」を「及び」に改め、「及び第四項」を削る部分に限る。）、同法第百六十九条の五第三項を削る改正規定、同法第百七十一条及び第百八十三条第二項の改正規定、同法第二百十六条の改正規定（「、第二十条第一項及び第二項（印鑑の提出）」を削り、「第十一号及び第十二号」を「第十号及び第十一号」に改める部分及び「において」の下に「、同法第十二条第一項第五号中「会社更生法（平成十四年法律第百五十四号）」とあるのは「金融機関等の更生手続の特例等に関する法律」と」を加える部分を除く。）並びに同法第三百三十三条第一項第十七号の次に一号を加える改正規定、第四十三条中金融機関等の更生手続の特例等に関する法律第百六十二条第一項後段を削る改正規定並びに同法第三百三十五条第一項後段及び第三百五十五条第一項後段を削る改正規定、第四十五条中資産の流動化に関する法律第二十二条第二項第七号の次に一号を加える改正規定、同条第四項を削る改正規定、同法第六十五条第三項の改正規定、同法第百八十三条第一項の改正規定（「第二十七条」を「第十九条の三」に、「、印鑑の提出、」を「）、第二十一条から第二十七条まで（」に改める部分、「、同法第二十四条第七号中「書面若しくは第三十条第二項若しくは第三十一条第二項に規定する譲渡人の承諾書」とあるのは「書面」と」を削る部分及び「準用する会社法第五百七条第三項」と」の下に「、同法第百四十六条の二中「商業登記法（」とあるのは「資産の流動化に関する法律（平成十年法律第百五号）第百八十三条第一項において準用する商業登記法（」と、「商業登記法第百四十五条」とあるのは「資産の流動化に関する法律第百八十三条第一項において準用する商業登記法第百四十五条」と」を加える部分を除く。）及び同法第三百十六条第一項第十七号の次に一号を加える改正規定、第四十八条の規定、第五十条中政党交付金の交付を受ける政党等に対する法人格の付与に関する法律第十五条の三の改正規定（「（第三項を除く。）」を削る部分に限る。）、第五十二条、第五十三条及び第五十五条の規定、第五十六条中酒税の保全及び酒類業組合等に関する法律第二十二条の改正規定（「、同法第九百三十七条第一項中「第九百三十条第二項各号」とあるのは「酒税の保全及び酒類業組合等に関する法律第六十七条第二項各号」と」を削る部分に限る。）、同法第三十九条、第五十六条第六項、第五十七条及び第六十七条から第六十九条までの改正規定、同法第七十八条の改正規定（前号に掲げる部分を除く。）並びに同法第八十三条の改正規定、第五十八条及び第六十一条の規定、第六十七条の規定（前号に掲げる改正規定を除く。）、第六十九条中消費生活協同組合法第八十一条から第八十三条まで及び第九十条第四項の改正規定並びに同法第九十二条の改正規定（前号に掲げる部分を除く。）、第七十一条中医療法第四十六条の三の六及び第七十条の二十一第六項の改正規定並びに同法第九十三条の改正規定（同条第四号中「第五十一条の三」を「第五十一条の三第一項」に改める部分を除く。）、第七十七条の規定、第八十条中農村負債整理組合法第二十四条第一項の改正規定（「第十七条（第三項ヲ除ク）」を「第十七条」に改める部分に限る。）、第八十一条中農業協同組合法第三十六条第七項の改正規定、同法第四十三条の六の次に一条を加える改正規定、同法第四十三条の七第三項の改正規定及び同法第百一条第一項第四十号の次に一号を加える改正規定、第八十三条中水産業協同組合法第四十条第七項の改正規定、同法第四十七条の五の次に一条を加える改正規定、同法第八十六条第二項の改正規定及び同法第百三十条第一項第三十八号の次に一号を加える改正規定、第八十五条中漁船損害等補償法第七十一条から第七十三条までの改正規定及び同法第八十三条の改正規定（前号に掲げる部分を除く。）、第八十七条中森林組合法第五十条第七項の改正規定、同法第六十条の三の次に一条を加える改正規定、同法第六十条の四第三項及び第百条第二項の改正規定並びに同法第百二十二条第一項第十二号の次に一号を加える改正規定、第八十九条中農林中央金庫及び特定農水産業協同組合等による信用事業の再編及び強化に関する法律第二十二条第二項の改正規定、第九十条中農林中央金庫法第四十六条の三の次に一条を加える改正規定、同法第四十七条第三項の改正規定及び同法第百条第一項第十六号の次に一号を加える改正規定、第九十三条中中小企業等協同組合法の目次の改正規定、同法第四章第二節第一款及び第二款の款名を削る改正規定、同法第九十三条から第九十五条まで、第九十六条第四項及び第九十七条第一項の改正規定並びに同法第百三条の改正規定（「、第四十八条」を「、第五十一条」に、「並びに第百三十二条」を「、第百三十二条から第百三十七条まで並びに第百三十九条」に改める部分及び「、同法第四十八条第二項中「会社法第九百三十条第二項各号」とあるのは「中小企業等協同組合法第九十三条第二項各号」と」を削る部分に限る。）、第九十六条の規定（同条中商品先物取引法第十八条第二項の改正規定、同法第二十九条の改正規定（前号に掲げる部分に限る。）並びに同法第五十八条、第七十七条第二項及び第百四十四条の十一第二項の改正規定を除く。）、第九十八条中輸出入取引法第十九条第一項の改正規定（「第八項」の下に「、第三十八条の六」を加える部分を除く。）、第百条の規定（同条中中小企業団体の組織に関する法律第百十三条第一項第十三号の改正規定を除く。）、第百二条中技術研究組合法の目次の改正規定、同法第八章第二節の節名の改正規定、同章第三節、第百五十九条第三項から第五項まで及び第百六十条第一項の改正規定並びに同法第百六十八条の改正規定（「、第四十八条」を「、第五十一条」に、「並びに第百三十二条」を「、第百三十二条から第百三十七条まで並びに第百三十九条」に改め、「第四十八条第二項中「会社法第九百三十条第二項各号」とあるのは「技術研究組合法第百五十六条第二項各号」と、同法第五十条第一項、」を削る部分に限る。）、第百七条の規定（前号に掲げる改正規定を除く。）並びに第百十一条の規定（前号に掲げる改正規定を除く。）会社法改正法附則第一条ただし書に規定する規定の施行の日
