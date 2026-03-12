use std::sync::LazyLock;

use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemePalette {
    pub background: Color,
    pub foreground: Color,
    pub muted: Color,
    pub selection: Color,
    pub primary: Color,
    pub secondary: Color,
    pub tertiary: Color,
    pub quaternary: Color,
    pub quinary: Color,
    pub error: Color,
    pub background_soft: Color,
    pub foreground_soft: Color,
    pub muted_soft: Color,
    pub selection_soft: Color,
    pub primary_soft: Color,
    pub secondary_soft: Color,
    pub tertiary_soft: Color,
    pub quaternary_soft: Color,
    pub quinary_soft: Color,
    pub error_soft: Color,
}

impl ThemePalette {
    pub fn aura() -> Self {
        Self {
            background: Color::Rgb(0x15, 0x14, 0x1b),
            foreground: Color::Rgb(0xed, 0xec, 0xee),
            muted: Color::Rgb(0x6d, 0x6d, 0x6d),
            selection: Color::Rgb(0x3d, 0x37, 0x5e),
            primary: Color::Rgb(0xa2, 0x77, 0xff),
            secondary: Color::Rgb(0x61, 0xff, 0xca),
            tertiary: Color::Rgb(0xff, 0xca, 0x85),
            quaternary: Color::Rgb(0xf6, 0x94, 0xff),
            quinary: Color::Rgb(0x82, 0xe2, 0xff),
            error: Color::Rgb(0xff, 0x67, 0x67),
            background_soft: Color::Rgb(0x15, 0x14, 0x1b),
            foreground_soft: Color::Rgb(0xbd, 0xbd, 0xbd),
            muted_soft: Color::Rgb(0x6d, 0x6d, 0x6d),
            selection_soft: Color::Rgb(0x3d, 0x37, 0x5e),
            primary_soft: Color::Rgb(0x84, 0x64, 0xc6),
            secondary_soft: Color::Rgb(0x54, 0xc5, 0x9f),
            tertiary_soft: Color::Rgb(0xc7, 0xa0, 0x6f),
            quaternary_soft: Color::Rgb(0xc1, 0x7a, 0xc8),
            quinary_soft: Color::Rgb(0x6c, 0xb2, 0xc7),
            error_soft: Color::Rgb(0xc5, 0x58, 0x58),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub assistant_message_style: Style,
    pub user_message_style: Style,
    pub tool_style: Style,
    pub tool_fail_style: Style,
    pub fail_style: Style,
    pub markdown_heading_style: Style,
    pub markdown_inline_code_style: Style,
    pub markdown_code_block_style: Style,
    pub markdown_bullet_style: Style,
    pub markdown_quote_style: Style,
    pub log_style: Style,
    pub dim_style: Style,
    pub separator_style: Style,
    pub spinner_style: Style,
    pub status_head_style: Style,
    pub status_trail_style: Style,
    pub status_dim_style: Style,
    pub thinking_label_style: Style,
    pub thinking_style: Style,
    pub tool_name_style: Style,
}

impl Theme {
    pub fn aura() -> Self {
        let palette = ThemePalette::aura();
        Self {
            assistant_message_style: Style::new().fg(palette.foreground),
            user_message_style: Style::new().fg(palette.foreground).bg(palette.selection),
            tool_style: Style::new().fg(palette.tertiary).add_modifier(Modifier::DIM),
            tool_fail_style: Style::new().fg(palette.error),
            fail_style: Style::new().fg(palette.error).add_modifier(Modifier::BOLD),
            markdown_heading_style: Style::new().fg(palette.secondary).add_modifier(Modifier::BOLD),
            markdown_inline_code_style: Style::new().fg(palette.tertiary),
            markdown_code_block_style: Style::new()
                .fg(palette.tertiary_soft)
                .add_modifier(Modifier::DIM),
            markdown_bullet_style: Style::new().fg(palette.quinary).add_modifier(Modifier::BOLD),
            markdown_quote_style: Style::new().fg(palette.muted_soft),
            log_style: Style::new().fg(palette.muted_soft),
            dim_style: Style::new().fg(palette.foreground_soft).add_modifier(Modifier::DIM),
            separator_style: Style::new().fg(palette.muted_soft),
            spinner_style: Style::new().fg(palette.primary).add_modifier(Modifier::BOLD),
            status_head_style: Style::new().fg(palette.foreground).add_modifier(Modifier::BOLD),
            status_trail_style: Style::new().fg(palette.foreground_soft),
            status_dim_style: Style::new().fg(palette.muted_soft).add_modifier(Modifier::DIM),
            thinking_label_style: Style::new().fg(palette.quaternary).add_modifier(Modifier::BOLD),
            thinking_style: Style::new().fg(palette.foreground_soft).add_modifier(Modifier::DIM),
            tool_name_style: Style::new().fg(palette.quinary),
        }
    }
}

static AURA: LazyLock<Theme> = LazyLock::new(Theme::aura);

pub fn current() -> &'static Theme {
    &AURA
}

pub fn user_message_style() -> Style {
    current().user_message_style
}

pub fn assistant_message_style() -> Style {
    current().assistant_message_style
}

pub fn tool_style() -> Style {
    current().tool_style
}

pub fn tool_fail_style() -> Style {
    current().tool_fail_style
}

pub fn fail_style() -> Style {
    current().fail_style
}

pub fn markdown_heading_style() -> Style {
    current().markdown_heading_style
}

pub fn markdown_inline_code_style() -> Style {
    current().markdown_inline_code_style
}

pub fn markdown_code_block_style() -> Style {
    current().markdown_code_block_style
}

pub fn markdown_bullet_style() -> Style {
    current().markdown_bullet_style
}

pub fn markdown_quote_style() -> Style {
    current().markdown_quote_style
}

pub fn log_style() -> Style {
    current().log_style
}

pub fn dim_style() -> Style {
    current().dim_style
}

pub fn separator_style() -> Style {
    current().separator_style
}

pub fn spinner_style() -> Style {
    current().spinner_style
}

pub fn status_head_style() -> Style {
    current().status_head_style
}

pub fn status_trail_style() -> Style {
    current().status_trail_style
}

pub fn status_dim_style() -> Style {
    current().status_dim_style
}

pub fn thinking_label_style() -> Style {
    current().thinking_label_style
}

pub fn thinking_style() -> Style {
    current().thinking_style
}

pub fn tool_name_style() -> Style {
    current().tool_name_style
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;

    use super::{Theme, ThemePalette, current};

    #[test]
    fn aura_调色板映射关键颜色() {
        let palette = ThemePalette::aura();

        assert_eq!(palette.background, Color::Rgb(0x15, 0x14, 0x1b));
        assert_eq!(palette.foreground, Color::Rgb(0xed, 0xec, 0xee));
        assert_eq!(palette.primary, Color::Rgb(0xa2, 0x77, 0xff));
        assert_eq!(palette.secondary, Color::Rgb(0x61, 0xff, 0xca));
        assert_eq!(palette.tertiary, Color::Rgb(0xff, 0xca, 0x85));
        assert_eq!(palette.quaternary, Color::Rgb(0xf6, 0x94, 0xff));
        assert_eq!(palette.quinary, Color::Rgb(0x82, 0xe2, 0xff));
        assert_eq!(palette.error, Color::Rgb(0xff, 0x67, 0x67));
        assert_eq!(palette.selection, Color::Rgb(0x3d, 0x37, 0x5e));
    }

    #[test]
    fn aura_主题映射关键语义样式() {
        let theme = Theme::aura();

        assert_eq!(theme.assistant_message_style.fg, Some(Color::Rgb(0xed, 0xec, 0xee)));
        assert_eq!(theme.assistant_message_style.bg, None);
        assert_eq!(theme.user_message_style.fg, Some(Color::Rgb(0xed, 0xec, 0xee)));
        assert_eq!(theme.user_message_style.bg, Some(Color::Rgb(0x3d, 0x37, 0x5e)));
        assert_eq!(theme.tool_name_style.fg, Some(Color::Rgb(0x82, 0xe2, 0xff)));
        assert_eq!(theme.thinking_label_style.fg, Some(Color::Rgb(0xf6, 0x94, 0xff)));
        assert_eq!(theme.fail_style.fg, Some(Color::Rgb(0xff, 0x67, 0x67)));
        assert_eq!(theme.separator_style.fg, Some(Color::Rgb(0x6d, 0x6d, 0x6d)));
        assert_eq!(theme.status_head_style.fg, Some(Color::Rgb(0xed, 0xec, 0xee)));
        assert_eq!(theme.status_trail_style.fg, Some(Color::Rgb(0xbd, 0xbd, 0xbd)));
    }

    #[test]
    fn 当前主题默认使用_aura() {
        let theme = current();
        assert_eq!(theme.user_message_style.bg, Some(ThemePalette::aura().selection));
        assert_eq!(theme.tool_name_style.fg, Some(ThemePalette::aura().quinary));
    }
}
