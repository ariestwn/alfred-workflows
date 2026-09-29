use crate::catalog::Catalog;
use crate::jev::{emoji_question, leaves, rank, subgroup_question};
use crate::{merge, paginate};
use serde_json::json;

#[test]
fn pages_fit_four_rows_of_eight() {
    let items: Vec<_> = (0..70).map(|i| json!({ "arg": i })).collect();
    let first = paginate(items.clone(), 0, "");
    assert_eq!(first.len(), 31);
    assert_eq!(first[30]["variables"]["EMOJI_PAGE"], "1");
    let middle = paginate(items.clone(), 1, "");
    assert_eq!(middle.len(), 32);
    assert_eq!(middle[0]["arg"], 30);
    let last = paginate(items.clone(), 99, "");
    assert_eq!(last.len(), 11);
    assert_eq!(last[10]["title"], "Previous");
    assert_eq!(paginate(items[..5].to_vec(), 0, "").len(), 5);
}

fn index(c: &Catalog, glyph: &str) -> usize {
    c.find(glyph).unwrap()
}

#[test]
fn catalog_has_base_emoji_without_skin_tones() {
    let c = Catalog::load();
    assert!(c.emoji.len() > 1800);
    assert!(c.emoji.iter().all(|e| !e.glyph.chars().any(|ch| ('\u{1F3FB}'..='\u{1F3FF}').contains(&ch))));
    assert_eq!(c.emoji[0].glyph, "😀");
    assert_eq!(c.emoji[0].icon(), "icons/1f600.png");
}

#[test]
fn keyword_search_uses_cldr_keywords() {
    let c = Catalog::load();
    let hits: Vec<usize> = c.keyword_search("happy").into_iter().map(|h| h.0).collect();
    assert!(hits.contains(&index(&c, "😀")));
    let hits = c.keyword_search("party face");
    assert!(hits.iter().take(3).any(|h| h.0 == index(&c, "🥳")));
    assert!(c.keyword_search("zzqqxx").is_empty());
}

#[test]
fn exact_name_ranks_first() {
    let c = Catalog::load();
    let hits = c.keyword_search("cat");
    assert_eq!(hits[0], (index(&c, "🐈"), true));
}

#[test]
fn every_leaf_fits_a_choice_and_covers_catalog() {
    let c = Catalog::load();
    let leaves = leaves(&c);
    assert!(leaves.len() < 255);
    assert!(leaves.iter().all(|l| l.members.len() + 1 <= 255));
    assert_eq!(leaves.iter().map(|l| l.members.len()).sum::<usize>(), c.emoji.len());
    let q = subgroup_question(&c, &leaves, crate::jev::READINGS[1].1);
    assert_eq!(q["criteria"].as_object().unwrap().len(), leaves.len());
    for leaf in &leaves {
        let q = emoji_question(&c, leaf);
        assert_eq!(q["criteria"].as_object().unwrap().len(), leaf.members.len() + 1);
    }
}

#[test]
fn rank_uses_geometric_mean_and_skips_none() {
    let c = Catalog::load();
    let leaves = leaves(&c);
    let hat = leaves.iter().find(|l| l.label.ends_with("face-hat")).unwrap();
    let event = leaves.iter().find(|l| l.label.ends_with("event")).unwrap();
    let beams = vec![(hat, 0.6), (event, 0.3)];
    let picks = vec![
        vec![("partying face".to_string(), 0.9), ("none".to_string(), 0.1)],
        vec![("party popper".to_string(), 0.8), ("balloon".to_string(), 0.001)],
    ];
    let ranked = rank(&c, &beams, &picks, 10, 0.05);
    assert_eq!(ranked.iter().map(|r| r.0).collect::<Vec<_>>(), vec![index(&c, "🥳"), index(&c, "🎉")]);
    assert!((ranked[0].1 - (0.6f64 * 0.9).sqrt()).abs() < 1e-9);
}

#[test]
fn merge_orders_exact_then_ai_then_keyword() {
    assert_eq!(merge(&[(5, false), (1, true)], &[9, 5]), vec![1, 9, 5]);
}
