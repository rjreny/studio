//! Live 627-film library regression. Ignored in CI (needs the installed DB + TMDB).
//! `cargo test --lib inspect_real_library -- --ignored --nocapture`
//! `TASTE_WRITE_FIXTURE=1` refreshes `fixtures/library_627.json`.
//! `TASTE_SKIP_ANALYZE=1` skips Call 1/Call 2.

use crate::storage::db::Database;
use crate::taste::features::{keyword_role, FeatureFamily, KeywordRole};
use crate::taste::library_fixture::snapshot_from_record;
use crate::taste::retrieve::{
    attach_signals, enrich_missing, enrich_rated_library, load_films, retrieve, seen_keys,
    RetrievalKind,
};
use crate::taste::score::{
    filmography_supported, person_pipeline_trace, reasons_are_genre_only,
    reasons_have_strong_bridge, score_all,
};
use crate::taste::shortlist::shortlist;
use crate::taste::validate::diversity_warnings;
use crate::taste::{analyze, feature_profile_from_films};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn candidate_dbs() -> Vec<PathBuf> {
    let roaming = std::env::var("APPDATA").unwrap_or_default();
    [
        "com.rjreny.studio",
        "com.local.studio",
    ]
    .into_iter()
    .map(|id| PathBuf::from(&roaming).join(id).join("studio.db"))
    .filter(|p| p.exists())
    .collect()
}

fn copy_db(src: &Path) -> PathBuf {
    let dir = std::env::temp_dir().join("studio-taste-inspect");
    let _ = std::fs::create_dir_all(&dir);
    let dest = dir.join("studio.db");
    std::fs::copy(src, &dest).expect("copy db");
    for suffix in ["-wal", "-shm"] {
        let side = PathBuf::from(format!("{}{suffix}", src.display()));
        if side.exists() {
            let _ = std::fs::copy(&side, dir.join(format!("studio.db{suffix}")));
        }
    }
    dest
}

#[ignore]
#[test]
fn inspect_real_library() {
    let src = candidate_dbs()
        .into_iter()
        .max_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        .expect("no studio.db under AppData");
    println!("SOURCE_DB {}", src.display());
    let dest = copy_db(&src);
    let db = Database::open(&dest).expect("open copied db");

    let mut films = load_films(&db).expect("load_films");
    let rated = films.iter().filter(|f| f.rating.is_some()).count();
    println!("RATED {rated}");
    assert!(
        rated >= 8,
        "copied library has {rated} ratings; expected a real log"
    );

    let hydrated = enrich_rated_library(&db, &mut films, 40);
    println!("HYDRATED_RATED {hydrated}");
    attach_signals(&mut films);
    if std::env::var("TASTE_WRITE_FIXTURE").as_deref() == Ok("1") {
        let snaps: Vec<_> = films.iter().map(snapshot_from_record).collect();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/taste/fixtures/library_627.json");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&path, serde_json::to_vec(&snaps).expect("serialize fixture"))
            .expect("write library_627.json");
        println!("WROTE_FIXTURE {} films -> {}", snaps.len(), path.display());
    }
    let profile = feature_profile_from_films(&films);
    let seen = seen_keys(&films);

    println!("--- affinities ---");
    for a in profile.affinities.iter().filter(|a| a.citeable()).take(16) {
        println!(
            "CITE {:?} {} n={} rec={:.2} conf={:.2} sa={:.2} cluster={:?}",
            a.key.family,
            a.key.name,
            a.appearances,
            a.recommendation_mean,
            a.confidence,
            a.scoring_affinity(),
            a.evidence_cluster
        );
    }
    println!("--- non-citeable people (first 12) ---");
    for a in profile
        .affinities
        .iter()
        .filter(|a| a.key.is_person_or_keyword() && !a.citeable())
        .take(12)
    {
        println!(
            "HIDE {:?} {} n={} rec={:.2} evidence={:?}",
            a.key.family,
            a.key.name,
            a.appearances,
            a.recommendation_mean,
            a.positive_evidence
                .iter()
                .map(|e| e.title.as_str())
                .collect::<Vec<_>>()
        );
    }

    let powell = profile.affinities.iter().find(|a| a.key.name == "John Powell");
    if let Some(a) = powell {
        println!(
            "POWELL n={} rec={:.3} conf={:.3} sa={:.3} cite={} evidence={:?} cluster={:?}",
            a.appearances,
            a.recommendation_mean,
            a.confidence,
            a.scoring_affinity(),
            a.citeable(),
            a.positive_evidence
                .iter()
                .map(|e| e.title.as_str())
                .collect::<Vec<_>>(),
            a.evidence_cluster
        );
    } else {
        println!("POWELL absent from profile");
    }

    for name in ["Stephen Hillenburg", "Greig Fraser", "Wally Pfister", "Bill Condon"] {
        match profile.affinities.iter().find(|a| a.key.name == name) {
            Some(a) => println!(
                "PERSON {name} n={} rec={:.2} cite={} evidence={:?}",
                a.appearances,
                a.recommendation_mean,
                a.citeable(),
                a.positive_evidence
                    .iter()
                    .map(|e| e.title.as_str())
                    .collect::<Vec<_>>()
            ),
            None => println!("PERSON {name} absent"),
        }
    }

    let decade = profile
        .affinities
        .iter()
        .find(|a| a.key.family == FeatureFamily::Decade && a.key.name == "2000s");
    if let Some(a) = decade {
        println!(
            "DECADE_2000s cite={} rec={:.2} port={:.2} n={}",
            a.citeable(),
            a.recommendation_mean,
            a.portability,
            a.appearances
        );
    }

    println!(
        "MODES {:?}",
        profile.modes.iter().map(|m| &m.dimension).collect::<Vec<_>>()
    );

    let mut candidates = retrieve(&db, &films, &profile, &seen).expect("retrieve");
    let enriched = enrich_missing(&db, &mut candidates, 40);
    println!("RETRIEVED {} ENRICHED {enriched}", candidates.len());

    let powell_injected: Vec<_> = candidates
        .iter()
        .filter(|c| {
            c.credits.iter().any(|p| p.name == "John Powell")
                || c.sources.iter().any(|s| s.label == "John Powell")
        })
        .collect();
    let powell_facet = powell_injected
        .iter()
        .filter(|c| filmography_supported(&profile, c))
        .count();
    println!(
        "POWELL_INJECTED {} FACET_OK {powell_facet}",
        powell_injected.len()
    );
    for c in powell_injected.iter().take(24) {
        println!(
            "  powell-cand {} {:?} facet={} src={:?}",
            c.title,
            c.genres,
            filmography_supported(&profile, c),
            c.sources
                .iter()
                .map(|s| format!("{:?}:{}", s.kind, s.label))
                .collect::<Vec<_>>()
        );
    }

    let twilight_seeds: Vec<_> = candidates
        .iter()
        .filter(|c| {
            c.sources.iter().any(|s| {
                s.label.to_lowercase().contains("twilight")
                    || s.label.to_lowercase().contains("curious george")
            })
        })
        .map(|c| c.title.clone())
        .collect();
    println!("TWILIGHT_OR_GEORGE_RELATED {}", twilight_seeds.len());
    for t in twilight_seeds.iter().take(12) {
        println!("  related-seed {t}");
    }

    let scored = score_all(&profile, &candidates);
    let short = shortlist(&scored);
    println!("SCORED {} SHORTLIST {}", scored.len(), short.len());

    let traces = person_pipeline_trace(&profile, &candidates, &scored, &short);
    for t in traces.iter().filter(|t| t.injected > 0 || t.survived_mmr > 0) {
        if t.injected >= 4 || t.survived_mmr >= 2 || t.name == "John Powell" || t.name.contains("Fraser")
        {
            println!(
                "TRACE {} inj={} score={} mmr={} n={} rec={:.2} conf={:.2} sa={:.2}",
                t.name,
                t.injected,
                t.survived_score,
                t.survived_mmr,
                t.appearances,
                t.recommendation_mean,
                t.confidence,
                t.scoring_affinity
            );
        }
    }

    let mut person_n: HashMap<String, usize> = HashMap::new();
    let mut mode_n: HashMap<String, usize> = HashMap::new();
    println!("--- shortlist ---");
    for (i, c) in short.iter().enumerate() {
        for k in &c.person_keys {
            *person_n.entry(k.clone()).or_insert(0) += 1;
        }
        for m in &c.candidate.modes {
            *mode_n.entry(m.clone()).or_insert(0) += 1;
        }
        let tw = c.evidence.iter().any(|e| e.to_lowercase().contains("twilight"));
        let george = c.evidence.iter().any(|e| e.to_lowercase().contains("curious george"));
        println!(
            "{:02} {} ({:?}) persons={:?} reasons={:?} evidence={:?} modes={:?} sources={:?} twilight_ev={tw} george_ev={george}",
            i + 1,
            c.candidate.title,
            c.candidate.year,
            c.person_keys,
            c.reasons,
            c.evidence,
            c.candidate.modes,
            c.candidate.sources.iter().map(|s| format!("{:?}", s.kind)).collect::<Vec<_>>(),
        );
    }
    println!("SHORTLIST_PERSONS {person_n:?}");
    println!("SHORTLIST_MODES {mode_n:?}");
    let warnings = diversity_warnings(&short);
    println!(
        "WARNINGS {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    let watchlist_n = short.iter().filter(|c| c.candidate.watchlist).count();
    let watchlist_genre_only = short
        .iter()
        .filter(|c| c.candidate.watchlist && reasons_are_genre_only(&c.reasons))
        .count();
    let watchlist_empty_ev = short
        .iter()
        .filter(|c| c.candidate.watchlist && c.evidence.is_empty())
        .count();
    let non_wl_genre_only = short
        .iter()
        .filter(|c| !c.candidate.watchlist && reasons_are_genre_only(&c.reasons))
        .count();
    let non_wl_empty_ev = short
        .iter()
        .filter(|c| !c.candidate.watchlist && c.evidence.is_empty())
        .count();
    let related_genre_only = short
        .iter()
        .filter(|c| {
            c.candidate
                .sources
                .iter()
                .any(|s| s.kind == RetrievalKind::Related)
                && !c.candidate.watchlist
                && !reasons_have_strong_bridge(&c.reasons)
        })
        .count();
    let pre_1990_backed = short
        .iter()
        .filter(|c| {
            c.candidate.year.map(|y| y < 1990).unwrap_or(false)
                && reasons_have_strong_bridge(&c.reasons)
        })
        .count();
    let fraser_short = short
        .iter()
        .filter(|c| c.person_keys.iter().any(|p| p.contains("Fraser")))
        .count();
    let noir_short = short
        .iter()
        .filter(|c| {
            c.reasons
                .iter()
                .any(|r| r.to_lowercase().contains("neo-noir"))
        })
        .count();
    let noir_citeable = profile
        .affinities
        .iter()
        .find(|a| a.key.name == "neo-noir")
        .map(|a| a.citeable())
        .unwrap_or(false);
    let filmography_n = short
        .iter()
        .filter(|c| {
            c.candidate
                .sources
                .iter()
                .any(|s| s.kind == RetrievalKind::Filmography)
        })
        .count();
    let related_n = short
        .iter()
        .filter(|c| {
            c.candidate
                .sources
                .iter()
                .any(|s| s.kind == RetrievalKind::Related)
        })
        .count();
    let empty_ev = short.iter().filter(|c| c.evidence.is_empty()).count();
    let pre_1990 = short
        .iter()
        .filter(|c| c.candidate.year.map(|y| y < 1990).unwrap_or(false))
        .count();
    let twilight_n = short
        .iter()
        .filter(|c| {
            c.person_keys.is_empty()
                && c.evidence
                    .iter()
                    .any(|e| e.to_lowercase().contains("twilight"))
        })
        .count();
    let george_n = short
        .iter()
        .filter(|c| {
            c.person_keys.is_empty()
                && c.evidence
                    .iter()
                    .any(|e| e.to_lowercase().contains("curious george"))
        })
        .count();
    let stinger_n = short
        .iter()
        .filter(|c| {
            c.reasons
                .iter()
                .any(|r| r.to_lowercase().contains("stinger"))
        })
        .count();
    let location_n = short
        .iter()
        .filter(|c| {
            c.reasons.iter().any(|r| {
                let r = r.to_lowercase();
                r.contains("new york city") || r.contains("los angeles")
            })
        })
        .count();
    let generic_kw_n = short
        .iter()
        .filter(|c| {
            c.reasons.iter().any(|r| {
                let r = r.to_lowercase();
                r.contains("cartoon affinity") || r.contains("anti hero")
            })
        })
        .count();
    let max_person = person_n.values().copied().max().unwrap_or(0);
    let powell_mmr = traces
        .iter()
        .filter(|t| t.name == "John Powell")
        .map(|t| t.survived_mmr)
        .sum::<usize>();
    println!("--- health ---");
    println!(
        "WATCHLIST {watchlist_n}/{} FILMOGRAPHY {filmography_n} RELATED {related_n}",
        short.len()
    );
    println!(
        "WL_GENRE_ONLY {watchlist_genre_only} WL_EMPTY_EV {watchlist_empty_ev} NON_WL_GENRE_ONLY {non_wl_genre_only} NON_WL_EMPTY_EV {non_wl_empty_ev} RELATED_GENRE_ONLY {related_genre_only}"
    );
    println!(
        "FRASER_SHORT {fraser_short} NEO_NOIR_SHORT {noir_short} NEO_NOIR_CITEABLE {noir_citeable} PRE_1990_BACKED {pre_1990_backed}/{pre_1990}"
    );
    println!("EMPTY_EVIDENCE {empty_ev} PRE_1990 {pre_1990} MAX_PERSON {max_person}");
    println!(
        "TWILIGHT_EV {twilight_n} GEORGE_EV {george_n} STINGER {stinger_n} LOCATION {location_n} GENERIC_KW {generic_kw_n} POWELL_MMR {powell_mmr}"
    );
    println!(
        "KEYWORD_ROLES stinger={:?} basedon={:?} nyc={:?} cartoon={:?} noir={:?}",
        keyword_role("duringcreditsstinger"),
        keyword_role("based on novel or book"),
        keyword_role("new york city"),
        keyword_role("cartoon"),
        keyword_role("neo-noir")
    );

    let hill = profile
        .affinities
        .iter()
        .find(|a| a.key.name == "Stephen Hillenburg");
    if let Some(a) = hill {
        assert!(!a.citeable(), "Hillenburg must not be citeable, n={}", a.appearances);
    }
    let fraser = profile
        .affinities
        .iter()
        .find(|a| a.key.name == "Greig Fraser");
    assert!(
        fraser.map(|a| a.citeable()).unwrap_or(false),
        "Fraser should remain a citeable cinematographer"
    );
    let decade = profile
        .affinities
        .iter()
        .find(|a| a.key.family == FeatureFamily::Decade && a.key.name == "2000s");
    if let Some(a) = decade {
        assert!(!a.citeable(), "2000s must stay contextual");
    }
    assert!(twilight_n <= 2, "Twilight evidence {twilight_n}/50");
    assert_eq!(george_n, 0, "Curious George as a person-empty genre seed");
    assert!(
        fraser_short >= 1,
        "Fraser path dropped from the 50"
    );
    assert!(noir_citeable, "neo-noir must remain a useful keyword");
    assert_eq!(stinger_n, 0, "stinger reasons");
    assert_eq!(location_n, 0, "location keywords driving reasons");
    assert_eq!(generic_kw_n, 0, "cartoon/anti-hero driving reasons");
    assert!(powell_mmr <= 2, "Powell shortlist {powell_mmr}");
    assert!(max_person <= 3, "one person in {max_person} of 50");
    let weak_watchlist = short
        .iter()
        .filter(|c| c.candidate.watchlist && !reasons_have_strong_bridge(&c.reasons))
        .count();
    assert_eq!(
        weak_watchlist, 0,
        "watchlist+genre-only still in the 50: {weak_watchlist} (watchlist {watchlist_n})"
    );
    assert_eq!(
        related_genre_only, 0,
        "related+genre-only still in the 50: {related_genre_only}"
    );
    assert_eq!(keyword_role("neo-noir"), KeywordRole::Signal);

    println!("--- analyze (LLM final 12) ---");
    if std::env::var("TASTE_SKIP_ANALYZE").as_deref() == Ok("1") {
        println!("ANALYZE_SKIPPED");
        return;
    }
    match analyze(&db, &mut |_| {}) {
        Ok(report) => {
            println!("TITLE {}", report.title);
            println!("RATED_COUNT {}", report.rated_count);
            println!("SUMMARY {}", report.summary);
            println!("MODEL {}", report.model);
            for a in &report.affinities {
                println!("AFFINITY {} :: {}", a.label, a.evidence);
            }
            for a in &report.aversions {
                println!("AVERSION {} :: {}", a.label, a.evidence);
            }
            for d in &report.dimensions {
                println!("DIM {} :: {}", d.name, d.take);
            }
            for (i, p) in report.picks.iter().enumerate() {
                println!(
                    "PICK {:02} {} ({:?}) mode={:?} why={} reasons={:?} evidence={:?}",
                    i + 1,
                    p.title,
                    p.year,
                    p.mode,
                    p.why,
                    p.reasons,
                    p.evidence
                );
            }
        }
        Err(err) => println!("ANALYZE_ERR {err}"),
    }
}
