use crate::widget_guidelines::{
    widget_guideline_document_for_modules, widget_guideline_full, widget_guideline_overview,
};

#[test]
fn overview_comes_from_prompt_sections() {
    let overview = widget_guideline_overview();

    assert!(overview.contains("# Widget renderer — Visual Creation Suite"));
    assert!(overview.contains("## Modules"));
    assert!(overview.contains("## Core Design System"));
    assert!(overview.contains("## When nothing fits"));
    assert!(!overview.contains("## SVG setup"));
}

#[test]
fn diagram_module_assembles_expected_prompt_sections() {
    let document = widget_guideline_document_for_modules(&["diagram".to_string()]);

    assert!(document.contains("## SVG setup"));
    assert!(document.contains("## Color palette"));
    assert!(document.contains("## Diagram types"));
    assert!(!document.contains("## Charts (Chart.js)"));
    assert!(!document.contains("## Art and illustration"));
}

#[test]
fn interactive_module_assembles_expected_prompt_sections() {
    let document = widget_guideline_document_for_modules(&["interactive".to_string()]);

    assert!(document.contains("## UI components"));
    assert!(document.contains("### 1. Interactive explainer — learn how something works"));
    assert!(!document.contains("### 2. Compare options — decision making"));
    assert!(!document.contains("## Diagram types"));
}

#[test]
fn full_guideline_matches_joined_prompt_sections() {
    assert_eq!(
        widget_guideline_full(),
        [
            include_str!("../../prompts/widget_guidelines/intro.md").trim(),
            include_str!("../../prompts/widget_guidelines/core_design_system.md").trim(),
            include_str!("../../prompts/widget_guidelines/when_nothing_fits.md").trim(),
            include_str!("../../prompts/widget_guidelines/svg_setup.md").trim(),
            include_str!("../../prompts/widget_guidelines/art.md").trim(),
            include_str!("../../prompts/widget_guidelines/ui_common.md").trim(),
            include_str!("../../prompts/widget_guidelines/ui_interactive.md").trim(),
            include_str!("../../prompts/widget_guidelines/ui_compare.md").trim(),
            include_str!("../../prompts/widget_guidelines/ui_data_record.md").trim(),
            include_str!("../../prompts/widget_guidelines/color_palette.md").trim(),
            include_str!("../../prompts/widget_guidelines/charts.md").trim(),
            include_str!("../../prompts/widget_guidelines/diagram_types.md").trim(),
        ]
        .join("\n\n")
    );
}
