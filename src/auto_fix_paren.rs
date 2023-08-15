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
  let mut pattern: Vec<SplitPatternList> = Vec::new();
  while now_head < paren_info_lst.len() - 3 {
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

      // 最短の取得を許される最大の回数から取っていくようにする
      // 典型的なパターンでは最速で終わり、込み入った例外パターンではより多様な選択を検証できる
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
