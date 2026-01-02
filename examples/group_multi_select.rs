use dialoguer::{theme::ColorfulTheme, GroupMultiSelect};

fn main() {
    let selections = GroupMultiSelect::new()
        .with_theme(&ColorfulTheme::default())
        .with_prompt("Select installation targets")
        .group("claude-code", vec!["work (active)", "personal"])
        .group("opencode", vec!["default (active)", "experiments"])
        .group("goose", vec!["main"])
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
