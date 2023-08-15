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
/// 「何文字で分割するのか」を保持する。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitPattern {
  /// 現在採用している候補
  now: usize,
  /// 何文字で分割するのか、の候補
  pattern_lst: Vec<usize>,
}

/// 改め文や読み替え規定文に出現するカギカッコ付きの文章を、
/// 開きカギカッコと閉じカギカッコの非対応があっても分割する関数
pub async fn auto_fix_paren(text: &str) -> Option<Vec<String>> {
  let mut paren_info_lst = Vec::new();
  for (i, c) in text.chars().enumerate() {
    match c {
      '「' => paren_info_lst.push(ParenInfo {
        pos: i,
        v: Paren::Open,
      }),
      '」' => paren_info_lst.push(ParenInfo {
        pos: i,
        v: Paren::Close,
      }),
      _ => (),
    }
  }
  let mut now_head: usize = 0;
  let mut pattern: Vec<SplitPattern> = Vec::new();
  while now_head < paren_info_lst.len() - 3 {
    let len_max = (paren_info_lst.len() - now_head) / 2;
    let mut pattern_lst = Vec::new();
    for i in 1..len_max {
      let paren_lst_1 = &paren_info_lst[now_head..=now_head + i]
        .iter()
        .map(|info| info.clone().v)
        .collect::<Vec<_>>();
      let paren_lst_2 = &paren_info_lst[now_head + i + 1..=now_head + i * 2 + 1]
        .iter()
        .map(|info| info.clone().v)
        .collect::<Vec<_>>();
      if paren_lst_1 == paren_lst_2
        && paren_lst_1[0] == Paren::Open
        && paren_lst_1[paren_lst_1.len() - 1] == Paren::Close
        && paren_lst_2[0] == Paren::Open
        && paren_lst_2[paren_lst_2.len() - 1] == Paren::Close
      {
        pattern_lst.push(i + 1)
      }
    }
    if pattern_lst.is_empty() {
      // 次の分割候補がないのでトラックバックする
      if let Some(split_pattern) = pattern.pop() {
        if split_pattern.now < split_pattern.pattern_lst.len() - 1 {
          now_head -= split_pattern.pattern_lst[split_pattern.now] * 2;
          now_head += split_pattern.pattern_lst[split_pattern.now + 1] * 2;
          pattern.push(SplitPattern {
            now: split_pattern.now + 1,
            pattern_lst: split_pattern.pattern_lst,
          })
        } else {
          now_head -= split_pattern.pattern_lst[split_pattern.now] * 2;
        }
      } else {
        // 分割位置が定まらないためその旨を返す
        return None;
      }
    } else {
      let n = pattern_lst[0];
      now_head += n * 2;
      pattern.push(SplitPattern {
        now: 0,
        pattern_lst,
      });
    }
  }
  let mut v = Vec::new();
  let mut paren_pos = 0;
  let mut char_pos = 0;
  let chars = text.chars().collect::<Vec<_>>();
  for SplitPattern { now, pattern_lst } in pattern.iter() {
    let len = pattern_lst[*now];
    let p1_start = paren_info_lst[paren_pos].pos;
    let p1_end = paren_info_lst[paren_pos + len - 1].pos;
    let p2_start = paren_info_lst[paren_pos + len].pos;
    let p2_end = paren_info_lst[paren_pos + len * 2 - 1].pos;
    let s1 = &chars[char_pos..p1_start].iter().collect::<String>();
    let s2 = &chars[p1_start..=p1_end].iter().collect::<String>();
    let s3 = &chars[p1_end + 1..p2_start].iter().collect::<String>();
    let s4 = &chars[p2_start..=p2_end].iter().collect::<String>();
    paren_pos += len * 2;
    char_pos = p2_end + 1;
    v.push(s1.clone());
    v.push(s2.clone());
    v.push(s3.clone());
    v.push(s4.clone());
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
