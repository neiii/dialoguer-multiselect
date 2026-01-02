use dialoguer_multiselect::{theme::ColorfulTheme, GroupMultiSelect, ItemState};

fn main() {
    let selections = GroupMultiSelect::new()
        .with_theme(&ColorfulTheme::default())
        .with_prompt("Select installation targets")
        .group("claude-code", vec!["work (active)", "personal"])
        .group_with_states(
            "opencode",
            vec![
                (
                    "default (active)",
                    ItemState::Warning {
                        message: "2 agents incompatible".into(),
                    },
                ),
                ("experiments", ItemState::Normal),
            ],
        )
        .group_with_states(
            "goose",
            vec![(
                "main",
                ItemState::Disabled {
                    reason: "agents not supported".into(),
                },
            )],
        )
        .defaults(vec![vec![true, false], vec![true, false], vec![false]])
        .interact()
        .unwrap();

    println!("\nSelected indices per group: {:?}", selections);

    let group_names = ["claude-code", "opencode", "goose"];
    let items: [&[&str]; 3] = [
        &["work (active)", "personal"],
        &["default (active)", "experiments"],
        &["main"],
    ];

    for (g_idx, indices) in selections.iter().enumerate() {
        if !indices.is_empty() {
            let names: Vec<_> = indices.iter().map(|&i| items[g_idx][i]).collect();
            println!("{}: {:?}", group_names[g_idx], names);
        }
    }
}
