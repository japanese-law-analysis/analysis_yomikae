use async_recursion::async_recursion;
use std::collections::HashSet;
use tokio_stream::StreamExt;

/// カギカッコの種類
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Paren {
  Open,
  Close,
}

/// カギカッコの種類と位置情報を保持する
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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

/// 解析のために一時的に使うトークン
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseToken {
  KagiOpen(usize),
  KagiClose(usize),
  MaruOpen,
  MaruClose,
}

/// 改め文や読み替え規定文に出現するカギカッコ付きの文章を、
/// 開きカギカッコと閉じカギカッコの非対応があっても分割する関数
pub async fn auto_fix_paren(text: &str) -> Option<Vec<String>> {
  // 文字列から括弧類だけを抽出し、丸括弧内の鉤括弧を排除して構造を簡略化する操作
  let mut dump_paren_lst = Vec::new();
  for (i, c) in text.chars().peekable().enumerate() {
    match c {
      '「' => dump_paren_lst.push(ParseToken::KagiOpen(i)),
      '」' => dump_paren_lst.push(ParseToken::KagiClose(i)),
      '（' => dump_paren_lst.push(ParseToken::MaruOpen),
      '）' => dump_paren_lst.push(ParseToken::MaruClose),
      _ => (),
    }
  }
  let mut maru_paren_depth = 0;
  let mut paren_info_lst = Vec::new();
  let mut token_iter = dump_paren_lst.iter().peekable();
  loop {
    match token_iter.next() {
      Some(ParseToken::KagiOpen(pos)) => {
        match token_iter.peek() {
          Some(ParseToken::MaruClose) => {
            token_iter.next();
          }
          _ => (),
        };
        if maru_paren_depth == 0 {
          paren_info_lst.push(ParenInfo {
            pos: *pos,
            v: Paren::Open,
          })
        }
      }
      Some(ParseToken::KagiClose(pos)) => {
        if maru_paren_depth == 0 {
          paren_info_lst.push(ParenInfo {
            pos: *pos,
            v: Paren::Close,
          })
        }
      }
      Some(ParseToken::MaruOpen) => match token_iter.peek() {
        Some(ParseToken::KagiClose(_)) => {}
        _ => maru_paren_depth += 1,
      },
      Some(ParseToken::MaruClose) => {
        if maru_paren_depth > 0 {
          maru_paren_depth -= 1;
        }
      }
      None => break,
    }
  }
  println!("paren_info_lst: {paren_info_lst:?}");

  // あり得る分割パターンを生成し、評価関数によって一番適当そうなものを採用する
  // ただし、愚直に括弧間で分割できる・できないで生成すると2^(n - 1)個生成されてしまう
  // そこで「分割可能位置は開き鍵括弧と閉じ鉤括弧がこの順で隣り合っている箇所」
  // という制約を加えることで枝刈りを行う
  let split_point_lst = generate_split_pattern(&paren_info_lst).await;

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

// 「分割可能位置は開き鍵括弧と閉じ鉤括弧がこの順で隣り合っている箇所」
// という制約のもと括弧列を分割することができる次の箇所のリストを生成する関数
#[async_recursion]
async fn generate_split_pattern(lst: &[ParenInfo]) -> HashSet<Vec<Vec<ParenInfo>>> {
  let mut next_lst = Vec::new();
  let mut l = lst.clone().iter().enumerate().peekable();
  loop {
    match l.next() {
      Some((i, info)) => {
        if Paren::Close == info.v {
          if let Some((_, ParenInfo { v: Paren::Open, .. })) = l.peek() {
            next_lst.push(i)
          }
        }
      }
      _ => break,
    }
  }
  next_lst.push(lst.len() - 1);

  let mut next_lst_stream = tokio_stream::iter(next_lst);
  let mut set = HashSet::new();
  while let Some(next_pos) = next_lst_stream.next().await {
    if next_pos != lst.len() - 1 {
      println!("lst: {lst:?}, next_pos: {next_pos}");
      let l1 = &lst[0..=next_pos];
      let l2 = &lst[next_pos + 1..];
      generate_split_pattern(l2).await.iter().for_each(|v| {
        let mut l = vec![l1.to_vec()];
        let mut v = v.clone();
        l.append(&mut v);
        set.insert(l);
      });
    } else {
      set.insert(vec![lst.to_vec()]);
    }
  }
  set
}

#[tokio::test]
async fn check_generate_split_pattern_1() {
  let v = vec![Paren::Open, Paren::Close, Paren::Open, Paren::Close];
  let v = v
    .iter()
    .map(|v| ParenInfo {
      v: v.clone(),
      pos: 0,
    })
    .collect::<Vec<_>>();
  let mut set = HashSet::new();
  vec![
    vec![
      vec![Paren::Open, Paren::Close],
      vec![Paren::Open, Paren::Close],
    ],
    vec![vec![Paren::Open, Paren::Close, Paren::Open, Paren::Close]],
  ]
  .iter()
  .for_each(|v| {
    let v = v
      .iter()
      .map(|v| {
        v.iter()
          .map(|v| ParenInfo {
            v: v.clone(),
            pos: 0,
          })
          .collect::<Vec<_>>()
      })
      .collect::<Vec<_>>();
    set.insert(v);
  });
  assert_eq!(generate_split_pattern(&v).await, set)
}

// あ「い」」う「え」」お「か「き」く」け」こ「さ「し」す」せ」そ「た」ち「つ」て」と「な」に「ぬ」ね」の
#[tokio::test]
async fn check_generate_split_pattern_2() {
  use Paren::*;
  let v = vec![
    Open, Close, Close, Open, Close, Close, Open, Open, Close, Close, Close, Open, Open, Close,
    Close, Close, Open, Close, Open, Close, Close, Open, Close, Open, Close, Close,
  ];
  let v = v
    .iter()
    .map(|v| ParenInfo {
      v: v.clone(),
      pos: 0,
    })
    .collect::<Vec<_>>();
  // 2^7 = 128
  assert_eq!(generate_split_pattern(&v).await.len(), 128)
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

/*
#[tokio::test]
async fn check_auto_fix_paren11() {
  assert_eq!(
    auto_fix_paren("あ「い」う「え」」お「か「き」」」く")
      .await
      .unwrap(),
    vec![
      "あ".to_string(),
      "「い」".to_string(),
      "う".to_string(),
      "「え」」".to_string(),
      "お".to_string(),
      "「か「き」」」".to_string(),
      "く".to_string()
    ]
  );

*/

// あ「い」う「（（え））」お「か」き「く」け「こ「さ」「し「す」せ「（（そ））」た」ち「つ」「（（て））」と「な」に「（ぬ）」ね「の」は「ひ」ふ「へ」ほ「（ま）」み「む」め「も」」ら「り」る

//法人の施行日前に開始した事業年度における新租税特別措置法第四十二条の十二の二第三項の規定の適用については、同項中「及び第三編第二章」とあるのは「（同法第七十二条及び第七十四条を所得税法等の一部を改正する法律（平成二十六年法律第十号）附則第二十五条の規定によりなお従前の例によることとされる場合における同法第三条の規定による改正前の法人税法第百四十五条第一項において準用する場合を含む。）」と、「は、同法」とあるのは「は、法人税法」と、「と、同法第百四十四条中「と、」とあるのは「と、「法人税の額」とあるのは「法人税の額（租税特別措置法第四十二条の十二の二第一項（認定地方公共団体の寄附活用事業に関連する寄附をした場合の法人税額の特別控除）の規定により控除する金額がある場合には、当該金額を控除した金額）」と、」と、同法第百四十四条の二第一項中「対する法人税の額」とあるのは「対する法人税の額（租税特別措置法第四十二条の十二の二第一項（認定地方公共団体の寄附活用事業に関連する寄附をした場合の法人税額の特別控除）の規定により控除する金額がある場合には、当該金額を控除した金額。次項及び第三項において同じ。）」と、同法第百四十四条の四第一項第三号中「の規定」とあるのは「及び租税特別措置法第四十二条の十二の二第一項（認定地方公共団体の寄附活用事業に関連する寄附をした場合の法人税額の特別控除）の規定」と、同項第四号及び同条第二項第二号中「前節」とあるのは「前節及び租税特別措置法第四十二条の十二の二第一項」と、同法第百四十四条の六第一項第三号中「の規定」とあるのは「及び租税特別措置法第四十二条の十二の二第一項（認定地方公共団体の寄附活用事業に関連する寄附をした場合の法人税額の特別控除）の規定」と、同項第四号及び同条第二項第二号中「前節」とあるのは「前節及び租税特別措置法第四十二条の十二の二第一項」とする」とあるのは「とする」とする。
