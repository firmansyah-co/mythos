//! The Sequential Function Chart corpus (`tests/sfc/`), which the emulators run
//! (`scripts/logic-vm-on-emulators.sh`): each document lowers, and every step, transition and
//! action of an action block in it is published with the variable keeping its state (ADR-013
//! §2.17), each at its own `localId`; the values of its bodies in LD and FBD in their own names.

use mythos_logic_graphic::{lower_with_probes, At, Holds, Source};
use std::collections::BTreeSet;

/// The `localId`s of the elements named `tag` in `xml` (an `action` of an action block too).
fn ids(xml: &str, tag: &str) -> BTreeSet<u64> {
    let open = format!("<{tag} localId=\"");
    xml.match_indices(&open)
        .map(|(at, _)| {
            let rest = &xml[at + open.len()..];
            rest[..rest.find('"').unwrap()].parse().unwrap()
        })
        .collect()
}

#[test]
fn every_document_of_the_corpus_lowers_and_publishes_every_step_transition_and_action() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sfc");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("xml") {
            continue;
        }
        let xml = std::fs::read_to_string(&path).unwrap();
        let (_, probes) = lower_with_probes(
            &[Source {
                name: "corpus.xml",
                text: &xml,
            }],
            None,
        )
        .unwrap_or_else(|d| panic!("{}: {d:#?}", path.display()));
        // An action's or a condition's body in LD or FBD keeps its values in its own namespace.
        for p in probes.iter().filter(|p| p.body.is_some()) {
            match &p.holds {
                Holds::Variable { path: kept, .. } | Holds::Guarded { path: kept, .. } => {
                    assert!(kept.starts_with("__SFC_"), "{}: {p:?}", path.display())
                }
                Holds::On => {}
            }
        }
        let probes: Vec<_> = probes.into_iter().filter(|p| p.body.is_none()).collect();
        let at = |want: fn(&At) -> bool| -> BTreeSet<u64> {
            probes
                .iter()
                .filter(|p| want(&p.at))
                .map(|p| p.local_id)
                .collect()
        };
        let name = path.display();
        assert_eq!(at(|a| *a == At::Step), ids(&xml, "step"), "{name}");
        assert_eq!(at(|a| *a == At::StepTime), ids(&xml, "step"), "{name}");
        assert_eq!(
            at(|a| *a == At::Transition),
            ids(&xml, "transition"),
            "{name}"
        );
        assert_eq!(at(|a| *a == At::Action), ids(&xml, "action"), "{name}");
        // A transition's condition is kept where it was evaluated: guarded by its enable flag.
        for p in probes.iter().filter(|p| p.at == At::Transition) {
            assert!(
                matches!(&p.holds, Holds::Guarded { ran, .. } if ran.ends_with("_EN")),
                "{name}: {p:?}"
            );
        }
        seen += 1;
    }
    assert!(seen >= 3, "the corpus has {seen} documents");
}
