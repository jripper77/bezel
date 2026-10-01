//! The few texts the backend itself shows, in the app's language
//! (D-2026-09-30-release-polish-6): the tray menu, the filters of the
//! native file dialogs and the name of a theme started without one.
//! Everything in the window is translated by the UI.

use bezel_core::domain::clock::Language;

/// The backend's texts in one language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Texts {
    /// Tray: show the window.
    pub show: &'static str,
    /// Tray: hide the window.
    pub hide: &'static str,
    /// Tray: live mode on the screen.
    pub live: &'static str,
    /// Tray: quit.
    pub quit: &'static str,
    /// File dialogs: theme files.
    pub themes: &'static str,
    /// File dialogs: images.
    pub images: &'static str,
    /// File dialogs: images and videos for a screen.
    pub media: &'static str,
    /// File dialogs: a theme's video background (videos and animated GIFs).
    pub videos: &'static str,
    /// Name of a theme started without one.
    pub untitled: &'static str,
}

/// The backend's texts in `language`.
pub fn texts(language: Language) -> Texts {
    match language {
        Language::PortugueseBr => Texts {
            show: "Abrir o Bezel",
            hide: "Ocultar a janela",
            live: "Ao vivo na tela",
            quit: "Sair",
            themes: "Temas",
            images: "Imagens",
            media: "Imagens e vídeos",
            videos: "Vídeos e GIFs animados",
            untitled: "Sem título",
        },
        Language::English => Texts {
            show: "Open Bezel",
            hide: "Hide the window",
            live: "Live on the screen",
            quit: "Quit",
            themes: "Themes",
            images: "Images",
            media: "Images and videos",
            videos: "Videos and animated GIFs",
            untitled: "Untitled",
        },
    }
}

/// A language as the UI and the settings spell it.
pub fn language_slug(language: Language) -> &'static str {
    match language {
        Language::PortugueseBr => "pt-BR",
        Language::English => "en",
    }
}

/// The language spelled `slug` (see [`language_slug`]).
pub fn parse_language(slug: &str) -> Option<Language> {
    [Language::PortugueseBr, Language::English]
        .into_iter()
        .find(|l| language_slug(*l) == slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_follow_the_language() {
        let pt = texts(Language::PortugueseBr);
        assert_eq!((pt.show, pt.quit), ("Abrir o Bezel", "Sair"));
        assert_eq!((pt.hide, pt.live), ("Ocultar a janela", "Ao vivo na tela"));
        assert_eq!((pt.themes, pt.untitled), ("Temas", "Sem título"));
        let en = texts(Language::English);
        assert_eq!(
            (en.show, en.hide, en.live, en.quit),
            (
                "Open Bezel",
                "Hide the window",
                "Live on the screen",
                "Quit"
            )
        );
        assert_eq!((en.images, en.media), ("Images", "Images and videos"));
        assert_eq!(en.videos, "Videos and animated GIFs");
        assert_eq!(pt.videos, "Vídeos e GIFs animados");
    }

    #[test]
    fn languages_are_spelled_like_the_ui() {
        for language in [Language::PortugueseBr, Language::English] {
            assert_eq!(parse_language(language_slug(language)), Some(language));
        }
        assert_eq!(parse_language("de"), None);
        assert_eq!(parse_language("pt"), None);
    }
}
