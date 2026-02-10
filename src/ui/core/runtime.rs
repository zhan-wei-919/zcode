use super::geom::Pos;
use super::id::Id;
use super::input::{DragPayload, UiEvent};
use super::tree::{Node, Sense, UiTree};
use crate::core::event::{InputEvent, MouseButton, MouseEventKind};

#[derive(Debug, Clone)]
pub struct UiRuntimeOutput {
    pub events: Vec<UiEvent>,
    pub needs_redraw: bool,
}

impl UiRuntimeOutput {
    pub fn empty() -> Self {
        Self {
            events: Vec::new(),
            needs_redraw: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PressedState {
    button: MouseButton,
    start: Pos,
    click: Option<Id>,
    drag_source: Option<Node>,
}

#[derive(Debug, Clone)]
struct DragSession {
    source: Id,
    payload: Option<DragPayload>,
    over: Option<Id>,
}

#[derive(Debug, Default)]
pub struct UiRuntime {
    hovered: Option<Id>,
    pressed: Option<PressedState>,
    capture: Option<Id>,
    dragging: bool,
    drag: Option<DragSession>,
    last_pos: Option<Pos>,
}

impl UiRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn hovered(&self) -> Option<Id> {
        self.hovered
    }

    pub fn capture(&self) -> Option<Id> {
        self.capture
    }

    pub fn is_pressed(&self) -> bool {
        self.pressed.is_some()
    }

    pub fn drag_payload(&self) -> Option<&DragPayload> {
        self.drag.as_ref()?.payload.as_ref()
    }

    pub fn drag_over(&self) -> Option<Id> {
        self.drag.as_ref()?.over
    }

    pub fn last_pos(&self) -> Option<Pos> {
        self.last_pos
    }

    pub fn reset_pointer_state(&mut self) {
        self.pressed = None;
        self.capture = None;
        self.dragging = false;
        self.drag = None;
    }

    pub fn on_input(&mut self, input: &InputEvent, tree: &UiTree) -> UiRuntimeOutput {
        let mut out = UiRuntimeOutput::empty();

        let InputEvent::Mouse(me) = input else {
            return out;
        };

        let pos = Pos::new(me.column, me.row);
        self.last_pos = Some(pos);

        // Hover update (Moved / Drag / any mouse event that changes position).
        let next_hover = tree.hit_test_with_sense(pos, Sense::HOVER).map(|n| n.id);
        if next_hover != self.hovered {
            out.events.push(UiEvent::HoverChanged {
                from: self.hovered,
                to: next_hover,
                pos,
            });
            self.hovered = next_hover;
            out.needs_redraw = true;
        }

        match me.kind {
            MouseEventKind::Down(button) => {
                let click = tree.hit_test_with_sense(pos, Sense::CLICK).map(|n| n.id);
                let drag_source = tree.hit_test_with_sense(pos, Sense::DRAG_SOURCE).copied();
                self.pressed = Some(PressedState {
                    button,
                    start: pos,
                    click,
                    drag_source,
                });
                self.dragging = false;
                self.drag = None;
            }
            MouseEventKind::Up(button) => {
                let pressed = self.pressed.take();
                let drag = self.drag.take();

                if let Some(drag) = drag {
                    if pressed.is_some_and(|p| p.button == button)
                        || self.capture == Some(drag.source)
                    {
                        // Drop (if any) happens before DragEnd.
                        if let (Some(payload), Some(target)) = (drag.payload, drag.over) {
                            out.events.push(UiEvent::Drop {
                                payload,
                                target,
                                pos,
                            });
                        }
                        out.events.push(UiEvent::DragEnd {
                            id: drag.source,
                            pos,
                        });
                        out.needs_redraw = true;
                    }
                } else if let Some(pressed) = pressed {
                    if pressed.button == button {
                        if button == MouseButton::Right {
                            if let Some(id) = tree
                                .hit_test_with_sense(pos, Sense::CONTEXT_MENU)
                                .map(|n| n.id)
                            {
                                out.events.push(UiEvent::ContextMenu { id, pos });
                                out.needs_redraw = true;
                            }
                        } else if let Some(id) = pressed.click {
                            out.events.push(UiEvent::Click { id, button, pos });
                        }
                    }
                }

                self.dragging = false;
                self.capture = None;
            }
            MouseEventKind::Drag(_button) => {
                // We rely on crossterm's Drag events, but still apply a small threshold to avoid
                // accidental drags from a click jitter.
                let Some(pressed) = self.pressed else {
                    return out;
                };

                let dx = pos.x as i32 - pressed.start.x as i32;
                let dy = pos.y as i32 - pressed.start.y as i32;
                let dist = dx.unsigned_abs() + dy.unsigned_abs();

                let threshold = match pressed.drag_source {
                    Some(node) if matches!(node.kind, super::tree::NodeKind::Splitter { .. }) => 0,
                    _ => 2,
                };

                if !self.dragging && dist >= threshold {
                    let Some(source) = pressed.drag_source else {
                        // Not draggable.
                        return out;
                    };

                    self.dragging = true;
                    self.capture = Some(source.id);

                    let payload = drag_payload_from_node(&source);
                    self.drag = Some(DragSession {
                        source: source.id,
                        payload,
                        over: None,
                    });

                    out.events.push(UiEvent::DragStart {
                        id: source.id,
                        pos: pressed.start,
                    });
                    out.needs_redraw = true;
                }

                if self.dragging {
                    if let Some(drag) = &mut self.drag {
                        let over = drag.payload.as_ref().and_then(|payload| {
                            tree.hit_test_with_sense_where(pos, Sense::DROP_TARGET, |n| {
                                can_drop(payload, n)
                            })
                            .map(|n| n.id)
                        });

                        if over != drag.over {
                            drag.over = over;
                            out.needs_redraw = true;
                        }

                        out.events.push(UiEvent::DragMove {
                            id: drag.source,
                            pos,
                            delta: (dx as i16, dy as i16),
                        });
                        out.needs_redraw = true;
                    }
                }
            }
            MouseEventKind::Moved => {}
            MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
            | MouseEventKind::ScrollLeft
            | MouseEventKind::ScrollRight => {}
        }

        out
    }
}

fn drag_payload_from_node(node: &Node) -> Option<DragPayload> {
    match node.kind {
        super::tree::NodeKind::Tab { pane, tab_id } => Some(DragPayload::Tab {
            from_pane: pane,
            tab_id,
        }),
        super::tree::NodeKind::ExplorerRow { node_id } => {
            Some(DragPayload::ExplorerNode { node_id })
        }
        _ => None,
    }
}

fn can_drop(payload: &DragPayload, target: &Node) -> bool {
    matches!(
        (payload, target.kind),
        (
            DragPayload::Tab { .. },
            super::tree::NodeKind::TabBar { .. }
        ) | (
            DragPayload::Tab { .. },
            super::tree::NodeKind::EditorSplitDrop { .. }
        ) | (
            DragPayload::ExplorerNode { .. },
            super::tree::NodeKind::EditorArea { .. }
        ) | (
            DragPayload::ExplorerNode { .. },
            super::tree::NodeKind::ExplorerRow { .. }
        ) | (
            DragPayload::ExplorerNode { .. },
            super::tree::NodeKind::ExplorerFolderDrop { .. }
        )
    )
}

#[cfg(test)]
#[path = "../../../tests/unit/ui/core/runtime.rs"]
mod tests;
