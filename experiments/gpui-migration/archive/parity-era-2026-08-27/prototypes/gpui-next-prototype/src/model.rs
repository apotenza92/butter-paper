#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ShellVariant {
    #[default]
    Workbench,
    Focus,
    Review,
}

impl ShellVariant {
    pub const ALL: [Self; 3] = [Self::Workbench, Self::Focus, Self::Review];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Workbench => "Workbench",
            Self::Focus => "Focus",
            Self::Review => "Review",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Workbench => "Document-first desktop workspace",
            Self::Focus => "Low-chrome reading and markup",
            Self::Review => "Comments and revision workflow",
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Workbench => Self::Focus,
            Self::Focus => Self::Review,
            Self::Review => Self::Workbench,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ShellVariant;

    #[test]
    fn variant_cycle_visits_every_shell_and_wraps() {
        let first = ShellVariant::Workbench;
        let second = first.next();
        let third = second.next();

        assert_eq!(second, ShellVariant::Focus);
        assert_eq!(third, ShellVariant::Review);
        assert_eq!(third.next(), first);
    }

    #[test]
    fn every_variant_has_review_copy() {
        for variant in ShellVariant::ALL {
            assert!(!variant.label().is_empty());
            assert!(!variant.description().is_empty());
        }
    }
}
