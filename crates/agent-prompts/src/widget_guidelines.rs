macro_rules! markdown_widget_guideline {
    ($fn_name:ident, $path:literal) => {
        pub fn $fn_name() -> &'static str {
            include_str!($path).trim()
        }
    };
}

markdown_widget_guideline!(widget_guideline_intro, "../prompts/widget_guidelines/intro.md");
markdown_widget_guideline!(
    widget_guideline_core_design_system,
    "../prompts/widget_guidelines/core_design_system.md"
);
markdown_widget_guideline!(
    widget_guideline_when_nothing_fits,
    "../prompts/widget_guidelines/when_nothing_fits.md"
);
markdown_widget_guideline!(widget_guideline_svg_setup, "../prompts/widget_guidelines/svg_setup.md");
markdown_widget_guideline!(widget_guideline_art_section, "../prompts/widget_guidelines/art.md");
markdown_widget_guideline!(widget_guideline_ui_common, "../prompts/widget_guidelines/ui_common.md");
markdown_widget_guideline!(
    widget_guideline_ui_interactive,
    "../prompts/widget_guidelines/ui_interactive.md"
);
markdown_widget_guideline!(
    widget_guideline_ui_compare,
    "../prompts/widget_guidelines/ui_compare.md"
);
markdown_widget_guideline!(
    widget_guideline_ui_data_record,
    "../prompts/widget_guidelines/ui_data_record.md"
);
markdown_widget_guideline!(
    widget_guideline_color_palette,
    "../prompts/widget_guidelines/color_palette.md"
);
markdown_widget_guideline!(
    widget_guideline_charts_section,
    "../prompts/widget_guidelines/charts.md"
);
markdown_widget_guideline!(
    widget_guideline_diagram_types,
    "../prompts/widget_guidelines/diagram_types.md"
);

pub const WIDGET_GUIDELINE_MODULES: [&str; 5] =
    ["art", "mockup", "interactive", "chart", "diagram"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SectionId {
    Intro,
    CoreDesignSystem,
    WhenNothingFits,
    SvgSetup,
    Art,
    UiCommon,
    UiInteractiveExplainer,
    UiCompareOptions,
    UiDataRecord,
    ColorPalette,
    Charts,
    DiagramTypes,
}

const BASE_SECTION_IDS: [SectionId; 3] =
    [SectionId::Intro, SectionId::CoreDesignSystem, SectionId::WhenNothingFits];

fn section_content(id: SectionId) -> &'static str {
    match id {
        SectionId::Intro => widget_guideline_intro(),
        SectionId::CoreDesignSystem => widget_guideline_core_design_system(),
        SectionId::WhenNothingFits => widget_guideline_when_nothing_fits(),
        SectionId::SvgSetup => widget_guideline_svg_setup(),
        SectionId::Art => widget_guideline_art_section(),
        SectionId::UiCommon => widget_guideline_ui_common(),
        SectionId::UiInteractiveExplainer => widget_guideline_ui_interactive(),
        SectionId::UiCompareOptions => widget_guideline_ui_compare(),
        SectionId::UiDataRecord => widget_guideline_ui_data_record(),
        SectionId::ColorPalette => widget_guideline_color_palette(),
        SectionId::Charts => widget_guideline_charts_section(),
        SectionId::DiagramTypes => widget_guideline_diagram_types(),
    }
}

fn module_section_ids(name: &str) -> Option<&'static [SectionId]> {
    match name {
        "art" => Some(&[SectionId::Art]),
        "mockup" => {
            Some(&[SectionId::UiCommon, SectionId::UiCompareOptions, SectionId::UiDataRecord])
        }
        "interactive" => Some(&[SectionId::UiCommon, SectionId::UiInteractiveExplainer]),
        "chart" => Some(&[SectionId::UiCommon, SectionId::ColorPalette, SectionId::Charts]),
        "diagram" => Some(&[SectionId::SvgSetup, SectionId::ColorPalette, SectionId::DiagramTypes]),
        _ => None,
    }
}

fn compose_sections(section_ids: &[SectionId]) -> String {
    let mut ordered = Vec::new();
    for section_id in section_ids {
        if !ordered.contains(section_id) {
            ordered.push(*section_id);
        }
    }

    ordered.into_iter().map(section_content).collect::<Vec<_>>().join("\n\n")
}

pub fn widget_guideline_full() -> &'static str {
    Box::leak(
        [
            widget_guideline_intro(),
            widget_guideline_core_design_system(),
            widget_guideline_when_nothing_fits(),
            widget_guideline_svg_setup(),
            widget_guideline_art_section(),
            widget_guideline_ui_common(),
            widget_guideline_ui_interactive(),
            widget_guideline_ui_compare(),
            widget_guideline_ui_data_record(),
            widget_guideline_color_palette(),
            widget_guideline_charts_section(),
            widget_guideline_diagram_types(),
        ]
        .join("\n\n")
        .into_boxed_str(),
    )
}

pub fn widget_guideline_overview() -> String {
    compose_sections(&BASE_SECTION_IDS)
}

pub fn widget_guideline_document_for_modules(modules: &[String]) -> String {
    let mut section_ids = BASE_SECTION_IDS.to_vec();
    for module in modules {
        if let Some(module_sections) = module_section_ids(module) {
            section_ids.extend_from_slice(module_sections);
        }
    }
    compose_sections(&section_ids)
}

#[cfg(test)]
#[path = "../tests/widget_guidelines/mod.rs"]
mod tests;
