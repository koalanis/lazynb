//! A document relationship graph, Obsidian/Quartz style: notes are nodes,
//! `[[links]]` and shared `#tags` are edges. Laid out with a Fruchterman-
//! Reingold force simulation that visibly settles over a few seconds, and
//! drawn on a ratatui braille `Canvas` so straight edges read as smooth
//! vectors rather than blocky ASCII.

use crate::config::Config;
use crate::overlay::{Action, Overlay};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::symbols::Marker;
use ratatui::text::Span;
use ratatui::widgets::canvas::{Canvas, Line as CanvasLine, Points};
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;

#[derive(Clone, Copy, PartialEq)]
pub enum EdgeKind {
    Link,
    Tag,
}

struct Node {
    id: String,
    title: String,
    path: String,
    x: f64,
    y: f64,
    dx: f64,
    dy: f64,
}

pub struct Graph {
    nodes: Vec<Node>,
    edges: Vec<(usize, usize, EdgeKind)>,
    selected: usize,
    iterations: u32,
    max_iterations: u32,
    temp: f64,
    /// Ideal edge length (Fruchterman-Reingold's `k`).
    k: f64,
    show_tags: bool,
}

impl Graph {
    /// `raw` is `(id, title, path)` per node; `edges` indexes into it.
    pub fn new(raw: Vec<(String, String, String)>, edges: Vec<(usize, usize, EdgeKind)>) -> Self {
        let n = raw.len().max(1);
        // Seed positions on a circle so the layout unfolds deterministically.
        let radius = 40.0;
        let nodes = raw
            .into_iter()
            .enumerate()
            .map(|(i, (id, title, path))| {
                let a = i as f64 / n as f64 * std::f64::consts::TAU;
                Node {
                    id,
                    title,
                    path,
                    x: radius * a.cos(),
                    y: radius * a.sin(),
                    dx: 0.0,
                    dy: 0.0,
                }
            })
            .collect();
        // Spread the nodes over a ~100x100 space.
        let k = (10_000.0 / n as f64).sqrt();
        Graph {
            nodes,
            edges,
            selected: 0,
            iterations: 0,
            max_iterations: 300,
            temp: k,
            k,
            show_tags: true,
        }
    }

    /// One Fruchterman-Reingold iteration: all-pairs repulsion, edge
    /// attraction, then a temperature-limited move with a little cooling.
    fn step(&mut self) {
        let n = self.nodes.len();
        for nd in &mut self.nodes {
            nd.dx = 0.0;
            nd.dy = 0.0;
        }

        // Repulsion between every pair of nodes.
        for i in 0..n {
            for j in (i + 1)..n {
                let mut dx = self.nodes[i].x - self.nodes[j].x;
                let mut dy = self.nodes[i].y - self.nodes[j].y;
                let mut dist = (dx * dx + dy * dy).sqrt();
                if dist < 0.01 {
                    // Coincident nodes: nudge apart deterministically.
                    dx = ((i % 7) as f64 - 3.0) * 0.1 + 0.05;
                    dy = ((j % 5) as f64 - 2.0) * 0.1 + 0.05;
                    dist = (dx * dx + dy * dy).sqrt();
                }
                let force = self.k * self.k / dist;
                let (ux, uy) = (dx / dist, dy / dist);
                self.nodes[i].dx += ux * force;
                self.nodes[i].dy += uy * force;
                self.nodes[j].dx -= ux * force;
                self.nodes[j].dy -= uy * force;
            }
        }

        // Attraction along edges.
        for &(a, b, kind) in &self.edges {
            if kind == EdgeKind::Tag && !self.show_tags {
                continue;
            }
            let dx = self.nodes[a].x - self.nodes[b].x;
            let dy = self.nodes[a].y - self.nodes[b].y;
            let dist = (dx * dx + dy * dy).sqrt().max(0.01);
            let force = dist * dist / self.k;
            let (ux, uy) = (dx / dist, dy / dist);
            self.nodes[a].dx -= ux * force;
            self.nodes[a].dy -= uy * force;
            self.nodes[b].dx += ux * force;
            self.nodes[b].dy += uy * force;
        }

        // Apply, capped by the current temperature, with a gentle pull to the
        // origin so disconnected pieces don't drift off-canvas.
        for nd in &mut self.nodes {
            let disp = (nd.dx * nd.dx + nd.dy * nd.dy).sqrt().max(0.01);
            let step = disp.min(self.temp);
            nd.x += nd.dx / disp * step;
            nd.y += nd.dy / disp * step;
            nd.x *= 0.995;
            nd.y *= 0.995;
        }

        self.temp = (self.temp * 0.96).max(0.4);
        self.iterations += 1;
    }

    fn reheat(&mut self) {
        self.iterations = 0;
        self.temp = self.k;
    }

    /// Bounding box of the node cloud, padded, for the canvas coordinate space.
    fn bounds(&self) -> ([f64; 2], [f64; 2]) {
        let mut minx = f64::MAX;
        let mut maxx = f64::MIN;
        let mut miny = f64::MAX;
        let mut maxy = f64::MIN;
        for nd in &self.nodes {
            minx = minx.min(nd.x);
            maxx = maxx.max(nd.x);
            miny = miny.min(nd.y);
            maxy = maxy.max(nd.y);
        }
        if !minx.is_finite() {
            return ([-50.0, 50.0], [-50.0, 50.0]);
        }
        let padx = ((maxx - minx) * 0.12).max(8.0);
        let pady = ((maxy - miny) * 0.12).max(8.0);
        ([minx - padx, maxx + padx], [miny - pady, maxy + pady])
    }
}

impl Overlay for Graph {
    fn size(&self) -> (u16, u16) {
        (90, 85)
    }

    fn tick(&mut self) {
        // A few iterations per frame: fast enough to settle in a few seconds,
        // slow enough to watch it happen.
        for _ in 0..3 {
            if self.iterations < self.max_iterations {
                self.step();
            }
        }
    }

    fn animating(&self) -> bool {
        self.iterations < self.max_iterations
    }

    fn on_key(&mut self, key: KeyEvent) -> Action {
        let n = self.nodes.len();
        match key.code {
            KeyCode::Esc => Action::Close,
            KeyCode::Enter => self
                .nodes
                .get(self.selected)
                .map(|nd| Action::SelectNote(nd.path.clone()))
                .unwrap_or(Action::None),
            KeyCode::Down | KeyCode::Right | KeyCode::Tab | KeyCode::Char('j') => {
                if n > 0 {
                    self.selected = (self.selected + 1) % n;
                }
                Action::None
            }
            KeyCode::Up | KeyCode::Left | KeyCode::Char('k') => {
                if n > 0 {
                    self.selected = (self.selected + n - 1) % n;
                }
                Action::None
            }
            KeyCode::Char('t') => {
                self.show_tags = !self.show_tags;
                self.reheat();
                Action::None
            }
            KeyCode::Char('r') => {
                self.reheat();
                Action::None
            }
            _ => Action::None,
        }
    }

    fn render(&self, f: &mut Frame, area: Rect, cfg: &Config) {
        let sel_title = self
            .nodes
            .get(self.selected)
            .map(|n| n.title.as_str())
            .unwrap_or("(empty)");
        let links = self.edges.iter().filter(|e| e.2 == EdgeKind::Link).count();
        let tags = self.edges.len() - links;
        let block = Block::default()
            .title(format!(" Graph · {sel_title} "))
            .title_bottom(format!(
                " {} notes · {} links · {} tag-links{} · ↑↓ select · Enter open · t tags · r relayout · Esc ",
                self.nodes.len(),
                links,
                tags,
                if self.show_tags { "" } else { " (hidden)" }
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(cfg.accent));
        let inner = block.inner(area);
        f.render_widget(block, area);

        if self.nodes.is_empty() {
            return;
        }

        let (xb, yb) = self.bounds();
        let canvas = Canvas::default()
            .marker(Marker::Braille)
            .x_bounds(xb)
            .y_bounds(yb)
            .paint(|ctx| {
                // Edges first, beneath the node markers.
                for &(a, b, kind) in &self.edges {
                    if kind == EdgeKind::Tag && !self.show_tags {
                        continue;
                    }
                    let incident = a == self.selected || b == self.selected;
                    let color = if incident {
                        cfg.highlight
                    } else if kind == EdgeKind::Link {
                        Color::Gray
                    } else {
                        Color::DarkGray
                    };
                    ctx.draw(&CanvasLine {
                        x1: self.nodes[a].x,
                        y1: self.nodes[a].y,
                        x2: self.nodes[b].x,
                        y2: self.nodes[b].y,
                        color,
                    });
                }

                ctx.layer();
                let coords: Vec<(f64, f64)> = self.nodes.iter().map(|n| (n.x, n.y)).collect();
                ctx.draw(&Points {
                    coords: &coords,
                    color: cfg.focus,
                });

                // Node id labels; the selected one stands out.
                for (i, nd) in self.nodes.iter().enumerate() {
                    let style = if i == self.selected {
                        Style::default().fg(cfg.highlight)
                    } else {
                        Style::default().fg(Color::Gray)
                    };
                    // Strip the redundant `notebook:` prefix; show the bare id.
                    let label = nd.id.rsplit(':').next().unwrap_or(&nd.id);
                    ctx.print(nd.x, nd.y, Span::styled(format!(" {label}"), style));
                }
            });
        f.render_widget(canvas, inner);
    }
}
