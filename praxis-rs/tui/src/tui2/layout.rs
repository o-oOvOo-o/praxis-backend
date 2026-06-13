use ratatui::layout::Rect;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Edges {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

pub(crate) type Padding = Edges;

impl Edges {
    pub(crate) const ZERO: Self = Self {
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
    };

    pub(crate) const fn all(value: u16) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub(crate) const fn symmetric(horizontal: u16, vertical: u16) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    pub(crate) fn inset(self, area: Rect) -> Rect {
        let horizontal = self.left.saturating_add(self.right);
        let vertical = self.top.saturating_add(self.bottom);
        Rect {
            x: area.x.saturating_add(self.left),
            y: area.y.saturating_add(self.top),
            width: area.width.saturating_sub(horizontal),
            height: area.height.saturating_sub(vertical),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Split {
    pub first: Rect,
    pub second: Rect,
}

impl Axis {
    pub(crate) fn split_leading(self, area: Rect, amount: u16) -> Split {
        match self {
            Axis::Horizontal => {
                let first_width = amount.min(area.width);
                Split {
                    first: Rect {
                        width: first_width,
                        ..area
                    },
                    second: Rect {
                        x: area.x.saturating_add(first_width),
                        width: area.width.saturating_sub(first_width),
                        ..area
                    },
                }
            }
            Axis::Vertical => {
                let first_height = amount.min(area.height);
                Split {
                    first: Rect {
                        height: first_height,
                        ..area
                    },
                    second: Rect {
                        y: area.y.saturating_add(first_height),
                        height: area.height.saturating_sub(first_height),
                        ..area
                    },
                }
            }
        }
    }

    pub(crate) fn split_trailing(self, area: Rect, amount: u16) -> Split {
        match self {
            Axis::Horizontal => {
                let second_width = amount.min(area.width);
                Split {
                    first: Rect {
                        width: area.width.saturating_sub(second_width),
                        ..area
                    },
                    second: Rect {
                        x: area
                            .x
                            .saturating_add(area.width.saturating_sub(second_width)),
                        width: second_width,
                        ..area
                    },
                }
            }
            Axis::Vertical => {
                let second_height = amount.min(area.height);
                Split {
                    first: Rect {
                        height: area.height.saturating_sub(second_height),
                        ..area
                    },
                    second: Rect {
                        y: area
                            .y
                            .saturating_add(area.height.saturating_sub(second_height)),
                        height: second_height,
                        ..area
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::{Axis, Edges};

    #[test]
    fn inset_saturates_when_padding_is_larger_than_area() {
        let area = Rect::new(10, 20, 3, 2);
        let padded = Edges::all(4).inset(area);
        assert_eq!(padded, Rect::new(14, 24, 0, 0));
    }

    #[test]
    fn split_leading_clamps_to_area() {
        let split = Axis::Horizontal.split_leading(Rect::new(0, 0, 5, 1), 9);
        assert_eq!(split.first, Rect::new(0, 0, 5, 1));
        assert_eq!(split.second, Rect::new(5, 0, 0, 1));
    }
}
