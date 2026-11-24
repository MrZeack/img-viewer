//! Composition guides for photography and visual arts
use crate::cache::CompositionLine;
use egui::{Color32, Pos2, Rect, Stroke};

/// Different types of composition guides available in the viewer
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompositionType {
    /// Classic rule of thirds grid
    RuleOfThirds,
    /// Golden ratio based composition
    GoldenRatio,
    /// Diagonal composition lines
    Diagonal,
    /// Triangular composition guides
    Triangle,
    /// Golden spiral composition guide
    GoldenSpiral,
    /// Center horizontal and vertical lines
    CenterLines,
    /// Phi-based grid system
    PhiGrid,
    /// Classical art composition system
    DynamicSymmetry,
    /// Geometric composition base
    ArmatureOfRectangle,
    /// Square-within-rectangle guides
    Rabatment,
    /// Balanced triangular guides
    HarmoniousTriangles,
    /// Reverse diagonal with reciprocals
    SinisterDiagonal,
    /// Format-specific aspect ratio guides
    AspectRatioGuides,
}

impl CompositionType {
    /// Returns all available composition types
    pub fn all() -> &'static [CompositionType] {
        &[
            CompositionType::RuleOfThirds,
            CompositionType::GoldenRatio,
            CompositionType::Diagonal,
            CompositionType::Triangle,
            CompositionType::GoldenSpiral,
            CompositionType::CenterLines,
            CompositionType::PhiGrid,
            CompositionType::DynamicSymmetry,
            CompositionType::ArmatureOfRectangle,
            CompositionType::Rabatment,
            CompositionType::HarmoniousTriangles,
            CompositionType::SinisterDiagonal,
            CompositionType::AspectRatioGuides,
        ]
    }

    /// Returns the display name for the composition type
    pub fn name(&self) -> &'static str {
        match self {
            CompositionType::RuleOfThirds => "Rule Of Thirds",
            CompositionType::GoldenRatio => "Golden Ratio",
            CompositionType::Diagonal => "Diagonal",
            CompositionType::Triangle => "Triangle",
            CompositionType::GoldenSpiral => "Golden Spiral",
            CompositionType::CenterLines => "Center Lines",
            CompositionType::PhiGrid => "Phi Grid",
            CompositionType::DynamicSymmetry => "Dynamic Symmetry",
            CompositionType::ArmatureOfRectangle => "Armature Of Rectangle",
            CompositionType::Rabatment => "Rabatment",
            CompositionType::HarmoniousTriangles => "Harmonious Triangles",
            CompositionType::SinisterDiagonal => "Sinister Diagonal",
            CompositionType::AspectRatioGuides => "Aspect Ratio Guides",
        }
    }

    /// Returns the next composition type in the sequence
    pub fn next(&self) -> CompositionType {
        let all = Self::all();
        let current_index = all.iter().position(|&t| t == *self).unwrap_or(0);
        all[(current_index + 1) % all.len()]
    }

    /// Returns the previous composition type in the sequence
    pub fn prev(&self) -> CompositionType {
        let all = Self::all();
        let current_index = all.iter().position(|&t| t == *self).unwrap_or(0);
        all[(current_index + all.len() - 1) % all.len()]
    }
}

const PHI_INV: f32 = 0.618_034;

/// Generates composition lines for a given composition type and image rectangle
pub fn generate_composition_lines(
    composition_type: CompositionType,
    image_rect: Rect,
) -> Vec<CompositionLine> {
    let white = Color32::from_white_alpha(210);
    let stroke = Stroke::new(1.5_f32, white);
    let thin_stroke = Stroke::new(1.0_f32, Color32::from_white_alpha(175));

    match composition_type {
        CompositionType::RuleOfThirds => rule_of_thirds(image_rect, stroke),
        CompositionType::GoldenRatio => golden_ratio(image_rect, stroke),
        CompositionType::Diagonal => diagonal(image_rect, stroke),
        CompositionType::Triangle => triangle(image_rect, stroke),
        CompositionType::GoldenSpiral => golden_spiral(image_rect, stroke, thin_stroke),
        CompositionType::CenterLines => center_lines(image_rect, stroke),
        CompositionType::PhiGrid => phi_grid(image_rect, thin_stroke),
        CompositionType::DynamicSymmetry => dynamic_symmetry(image_rect, stroke, thin_stroke),
        CompositionType::ArmatureOfRectangle => {
            armature_of_rectangle(image_rect, stroke, thin_stroke)
        }
        CompositionType::Rabatment => rabatment(image_rect, stroke),
        CompositionType::HarmoniousTriangles => harmonious_triangles(image_rect, stroke),
        CompositionType::SinisterDiagonal => sinister_diagonal(image_rect, stroke, thin_stroke),
        CompositionType::AspectRatioGuides => aspect_ratio_guides(image_rect, thin_stroke),
    }
}

fn rule_of_thirds(rect: Rect, stroke: Stroke) -> Vec<CompositionLine> {
    let w3 = rect.left() + rect.width() / 3.0;
    let w23 = rect.left() + 2.0 * rect.width() / 3.0;
    let h3 = rect.top() + rect.height() / 3.0;
    let h23 = rect.top() + 2.0 * rect.height() / 3.0;

    vec![
        CompositionLine {
            start: Pos2::new(w3, rect.top()),
            end: Pos2::new(w3, rect.bottom()),
            stroke,
        },
        CompositionLine {
            start: Pos2::new(w23, rect.top()),
            end: Pos2::new(w23, rect.bottom()),
            stroke,
        },
        CompositionLine {
            start: Pos2::new(rect.left(), h3),
            end: Pos2::new(rect.right(), h3),
            stroke,
        },
        CompositionLine {
            start: Pos2::new(rect.left(), h23),
            end: Pos2::new(rect.right(), h23),
            stroke,
        },
    ]
}

fn golden_ratio(rect: Rect, stroke: Stroke) -> Vec<CompositionLine> {
    let v1 = rect.left() + rect.width() * PHI_INV;
    let v2 = rect.left() + rect.width() * (1.0 - PHI_INV);
    let h1 = rect.top() + rect.height() * PHI_INV;
    let h2 = rect.top() + rect.height() * (1.0 - PHI_INV);

    vec![
        CompositionLine {
            start: Pos2::new(v1, rect.top()),
            end: Pos2::new(v1, rect.bottom()),
            stroke,
        },
        CompositionLine {
            start: Pos2::new(v2, rect.top()),
            end: Pos2::new(v2, rect.bottom()),
            stroke,
        },
        CompositionLine {
            start: Pos2::new(rect.left(), h1),
            end: Pos2::new(rect.right(), h1),
            stroke,
        },
        CompositionLine {
            start: Pos2::new(rect.left(), h2),
            end: Pos2::new(rect.right(), h2),
            stroke,
        },
    ]
}

fn diagonal(rect: Rect, stroke: Stroke) -> Vec<CompositionLine> {
    vec![
        CompositionLine {
            start: Pos2::new(rect.left(), rect.top()),
            end: Pos2::new(rect.right(), rect.bottom()),
            stroke,
        },
        CompositionLine {
            start: Pos2::new(rect.right(), rect.top()),
            end: Pos2::new(rect.left(), rect.bottom()),
            stroke,
        },
    ]
}

fn triangle(rect: Rect, stroke: Stroke) -> Vec<CompositionLine> {
    let mid_x = rect.center().x;

    vec![
        CompositionLine {
            start: Pos2::new(mid_x, rect.top()),
            end: Pos2::new(rect.left(), rect.bottom()),
            stroke,
        },
        CompositionLine {
            start: Pos2::new(mid_x, rect.top()),
            end: Pos2::new(rect.right(), rect.bottom()),
            stroke,
        },
        CompositionLine {
            start: Pos2::new(rect.left(), rect.bottom()),
            end: Pos2::new(rect.right(), rect.bottom()),
            stroke,
        },
    ]
}

fn golden_spiral(rect: Rect, stroke: Stroke, thin_stroke: Stroke) -> Vec<CompositionLine> {
    use std::f32::consts::{FRAC_PI_2, PI, TAU};

    let mut lines = Vec::new();
    let mut cur = rect;
    const STEPS: usize = 6;
    const ARC_SEGMENTS: usize = 16;

    for step in 0..STEPS {
        let (l, r, t, b) = (cur.left(), cur.right(), cur.top(), cur.bottom());
        let (w, h) = (cur.width(), cur.height());
        if w < 2.0 || h < 2.0 {
            break;
        }
        let (cx, cy, rx, ry, start_angle, end_angle, next_rect) = match step % 4 {
            0 => {
                let sx = l + w * PHI_INV;
                lines.push(CompositionLine {
                    start: Pos2::new(sx, t),
                    end: Pos2::new(sx, b),
                    stroke: thin_stroke,
                });
                (
                    sx,
                    b,
                    sx - l,
                    b - t,
                    PI,
                    PI + FRAC_PI_2,
                    Rect::from_min_max(Pos2::new(sx, t), Pos2::new(r, b)),
                )
            }
            1 => {
                let sy = t + h * PHI_INV;
                lines.push(CompositionLine {
                    start: Pos2::new(l, sy),
                    end: Pos2::new(r, sy),
                    stroke: thin_stroke,
                });
                (
                    l,
                    sy,
                    r - l,
                    sy - t,
                    PI + FRAC_PI_2,
                    TAU,
                    Rect::from_min_max(Pos2::new(l, sy), Pos2::new(r, b)),
                )
            }
            2 => {
                let sx = r - w * PHI_INV;
                lines.push(CompositionLine {
                    start: Pos2::new(sx, t),
                    end: Pos2::new(sx, b),
                    stroke: thin_stroke,
                });
                (
                    sx,
                    t,
                    r - sx,
                    b - t,
                    0.0,
                    FRAC_PI_2,
                    Rect::from_min_max(Pos2::new(l, t), Pos2::new(sx, b)),
                )
            }
            _ => {
                let sy = b - h * PHI_INV;
                lines.push(CompositionLine {
                    start: Pos2::new(l, sy),
                    end: Pos2::new(r, sy),
                    stroke: thin_stroke,
                });
                (
                    r,
                    sy,
                    r - l,
                    b - sy,
                    FRAC_PI_2,
                    PI,
                    Rect::from_min_max(Pos2::new(l, t), Pos2::new(r, sy)),
                )
            }
        };

        let mut prev = Pos2::new(cx + rx * start_angle.cos(), cy + ry * start_angle.sin());
        for i in 1..=ARC_SEGMENTS {
            let frac = i as f32 / ARC_SEGMENTS as f32;
            let angle = start_angle + (end_angle - start_angle) * frac;
            let next = Pos2::new(cx + rx * angle.cos(), cy + ry * angle.sin());
            lines.push(CompositionLine {
                start: prev,
                end: next,
                stroke,
            });
            prev = next;
        }
        cur = next_rect;
    }

    lines
}

fn center_lines(rect: Rect, stroke: Stroke) -> Vec<CompositionLine> {
    let mid_x = rect.center().x;
    let mid_y = rect.center().y;

    vec![
        CompositionLine {
            start: Pos2::new(mid_x, rect.top()),
            end: Pos2::new(mid_x, rect.bottom()),
            stroke,
        },
        CompositionLine {
            start: Pos2::new(rect.left(), mid_y),
            end: Pos2::new(rect.right(), mid_y),
            stroke,
        },
    ]
}

fn phi_grid(rect: Rect, stroke: Stroke) -> Vec<CompositionLine> {
    let positions = [0.276, 0.447, PHI_INV];
    let mut lines = Vec::new();

    for &pos in &positions {
        let x = rect.left() + rect.width() * pos;
        let y = rect.top() + rect.height() * pos;

        lines.push(CompositionLine {
            start: Pos2::new(x, rect.top()),
            end: Pos2::new(x, rect.bottom()),
            stroke,
        });
        lines.push(CompositionLine {
            start: Pos2::new(rect.left(), y),
            end: Pos2::new(rect.right(), y),
            stroke,
        });
    }

    lines
}

fn dynamic_symmetry(rect: Rect, thick_stroke: Stroke, thin_stroke: Stroke) -> Vec<CompositionLine> {
    let (l, r, t, b) = (rect.left(), rect.right(), rect.top(), rect.bottom());
    let (w, h) = (rect.width(), rect.height());
    let mut lines = vec![
        // Baroque and sinister main diagonals
        CompositionLine {
            start: Pos2::new(l, t),
            end: Pos2::new(r, b),
            stroke: thick_stroke,
        },
        CompositionLine {
            start: Pos2::new(r, t),
            end: Pos2::new(l, b),
            stroke: thick_stroke,
        },
    ];

    if w > 0.0 && h > 0.0 {
        // Four 90-degree reciprocal lines from each corner to the opposite edge.
        let s = (w / h).min(h / w);
        let dx = s * h;
        let dy = s * w;
        lines.extend([
            CompositionLine {
                start: Pos2::new(l, b),
                end: Pos2::new(l + dx, b - dy),
                stroke: thin_stroke,
            },
            CompositionLine {
                start: Pos2::new(r, t),
                end: Pos2::new(r - dx, t + dy),
                stroke: thin_stroke,
            },
            CompositionLine {
                start: Pos2::new(l, t),
                end: Pos2::new(l + dx, t + dy),
                stroke: thin_stroke,
            },
            CompositionLine {
                start: Pos2::new(r, b),
                end: Pos2::new(r - dx, b - dy),
                stroke: thin_stroke,
            },
        ]);
    }

    lines
}

fn armature_of_rectangle(
    rect: Rect,
    thick_stroke: Stroke,
    thin_stroke: Stroke,
) -> Vec<CompositionLine> {
    let (l, r, t, b) = (rect.left(), rect.right(), rect.top(), rect.bottom());
    let mid_x = rect.center().x;
    let mid_y = rect.center().y;

    vec![
        // Main diagonals
        CompositionLine {
            start: Pos2::new(l, t),
            end: Pos2::new(r, b),
            stroke: thick_stroke,
        },
        CompositionLine {
            start: Pos2::new(r, t),
            end: Pos2::new(l, b),
            stroke: thick_stroke,
        },
        // Horizontal and vertical midlines
        CompositionLine {
            start: Pos2::new(mid_x, t),
            end: Pos2::new(mid_x, b),
            stroke: thin_stroke,
        },
        CompositionLine {
            start: Pos2::new(l, mid_y),
            end: Pos2::new(r, mid_y),
            stroke: thin_stroke,
        },
        // Diamond connecting edge midpoints
        CompositionLine {
            start: Pos2::new(mid_x, t),
            end: Pos2::new(r, mid_y),
            stroke: thin_stroke,
        },
        CompositionLine {
            start: Pos2::new(r, mid_y),
            end: Pos2::new(mid_x, b),
            stroke: thin_stroke,
        },
        CompositionLine {
            start: Pos2::new(mid_x, b),
            end: Pos2::new(l, mid_y),
            stroke: thin_stroke,
        },
        CompositionLine {
            start: Pos2::new(l, mid_y),
            end: Pos2::new(mid_x, t),
            stroke: thin_stroke,
        },
    ]
}

fn rabatment(rect: Rect, stroke: Stroke) -> Vec<CompositionLine> {
    if rect.width() > rect.height() {
        // Landscape
        vec![
            CompositionLine {
                start: Pos2::new(rect.left() + rect.height(), rect.top()),
                end: Pos2::new(rect.left() + rect.height(), rect.bottom()),
                stroke,
            },
            CompositionLine {
                start: Pos2::new(rect.right() - rect.height(), rect.top()),
                end: Pos2::new(rect.right() - rect.height(), rect.bottom()),
                stroke,
            },
        ]
    } else {
        // Portrait
        vec![
            CompositionLine {
                start: Pos2::new(rect.left(), rect.top() + rect.width()),
                end: Pos2::new(rect.right(), rect.top() + rect.width()),
                stroke,
            },
            CompositionLine {
                start: Pos2::new(rect.left(), rect.bottom() - rect.width()),
                end: Pos2::new(rect.right(), rect.bottom() - rect.width()),
                stroke,
            },
        ]
    }
}

fn harmonious_triangles(rect: Rect, stroke: Stroke) -> Vec<CompositionLine> {
    let mid_x = rect.center().x;
    let mid_y = rect.center().y;

    vec![
        CompositionLine {
            start: Pos2::new(rect.left(), rect.top()),
            end: Pos2::new(rect.right(), mid_y),
            stroke,
        },
        CompositionLine {
            start: Pos2::new(rect.right(), mid_y),
            end: Pos2::new(rect.left(), rect.bottom()),
            stroke,
        },
        CompositionLine {
            start: Pos2::new(rect.left(), rect.bottom()),
            end: Pos2::new(mid_x, rect.top()),
            stroke,
        },
        CompositionLine {
            start: Pos2::new(mid_x, rect.top()),
            end: Pos2::new(rect.right(), rect.bottom()),
            stroke,
        },
    ]
}

fn sinister_diagonal(
    rect: Rect,
    thick_stroke: Stroke,
    thin_stroke: Stroke,
) -> Vec<CompositionLine> {
    let w3 = rect.left() + rect.width() / 3.0;
    let w23 = rect.left() + 2.0 * rect.width() / 3.0;

    vec![
        CompositionLine {
            start: Pos2::new(rect.right(), rect.top()),
            end: Pos2::new(rect.left(), rect.bottom()),
            stroke: thick_stroke,
        },
        CompositionLine {
            start: Pos2::new(rect.left(), rect.top()),
            end: Pos2::new(w3, rect.bottom()),
            stroke: thin_stroke,
        },
        CompositionLine {
            start: Pos2::new(w23, rect.top()),
            end: Pos2::new(rect.right(), rect.bottom()),
            stroke: thin_stroke,
        },
    ]
}

fn aspect_ratio_guides(rect: Rect, stroke: Stroke) -> Vec<CompositionLine> {
    let ratio = rect.width() / rect.height();

    if (ratio - 16.0 / 9.0).abs() < 0.1 {
        // Cinematic - draw safe area
        let margin = 0.05;
        let safe_left = rect.left() + rect.width() * margin;
        let safe_right = rect.right() - rect.width() * margin;
        let safe_top = rect.top() + rect.height() * margin;
        let safe_bottom = rect.bottom() - rect.height() * margin;

        vec![
            CompositionLine {
                start: Pos2::new(safe_left, safe_top),
                end: Pos2::new(safe_right, safe_top),
                stroke,
            },
            CompositionLine {
                start: Pos2::new(safe_right, safe_top),
                end: Pos2::new(safe_right, safe_bottom),
                stroke,
            },
            CompositionLine {
                start: Pos2::new(safe_right, safe_bottom),
                end: Pos2::new(safe_left, safe_bottom),
                stroke,
            },
            CompositionLine {
                start: Pos2::new(safe_left, safe_bottom),
                end: Pos2::new(safe_left, safe_top),
                stroke,
            },
        ]
    } else {
        // Generic guides - quarter lines
        let w4 = rect.left() + rect.width() / 4.0;
        let w34 = rect.left() + 3.0 * rect.width() / 4.0;

        vec![
            CompositionLine {
                start: Pos2::new(w4, rect.top()),
                end: Pos2::new(w4, rect.bottom()),
                stroke,
            },
            CompositionLine {
                start: Pos2::new(w34, rect.top()),
                end: Pos2::new(w34, rect.bottom()),
                stroke,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_spiral_generates_curved_arc_segments() {
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0));
        let lines = generate_composition_lines(CompositionType::GoldenSpiral, rect);
        assert!(
            lines.len() > 30,
            "expected curved spiral polyline, got {} segments",
            lines.len()
        );
    }

    #[test]
    fn dynamic_symmetry_and_armature_are_distinct() {
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0));
        let dyn_sym = generate_composition_lines(CompositionType::DynamicSymmetry, rect);
        let armature = generate_composition_lines(CompositionType::ArmatureOfRectangle, rect);
        let to_pts = |lines: &[CompositionLine]| -> Vec<((i32, i32), (i32, i32))> {
            let mut v: Vec<_> = lines
                .iter()
                .map(|l| {
                    let a = (l.start.x.round() as i32, l.start.y.round() as i32);
                    let b = (l.end.x.round() as i32, l.end.y.round() as i32);
                    if a <= b {
                        (a, b)
                    } else {
                        (b, a)
                    }
                })
                .collect();
            v.sort();
            v
        };
        assert_ne!(
            to_pts(&dyn_sym),
            to_pts(&armature),
            "Dynamic Symmetry and Armature of Rectangle must not be identical"
        );
        assert!(dyn_sym.len() >= 6, "Dynamic Symmetry needs reciprocals");
        assert!(armature.len() >= 8, "Armature needs diamond/eye lines");
    }
}
