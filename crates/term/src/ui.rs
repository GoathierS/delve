//! Drawing the game with ratatui.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Gauge, Paragraph, Wrap};

use delve_core::game::{Ability, Game, State, THROW_COST, TORCH_MAX, Tone};
use delve_core::map::{self, Pos, Tile};

const WALL: Color = Color::Rgb(150, 140, 130);
const WALL_SEEN: Color = Color::Rgb(70, 65, 60);
const FLOOR: Color = Color::Rgb(90, 90, 100);
const FLOOR_SEEN: Color = Color::Rgb(40, 40, 48);
const HERO: Color = Color::Rgb(255, 230, 120);
const GOLD: Color = Color::Rgb(255, 200, 40);
const POTION: Color = Color::Rgb(240, 90, 200);
const OIL: Color = Color::Rgb(255, 150, 40);
const THREAT: Color = Color::Rgb(110, 15, 15);

pub fn draw(f: &mut Frame, game: &Game, aim: Option<Ability>) {
    let [top, log] = Layout::vertical([Constraint::Min(10), Constraint::Length(8)]).areas(f.area());
    let [map_area, side] = Layout::horizontal([Constraint::Min(20), Constraint::Length(28)]).areas(top);

    let title = match aim {
        Some(a) => format!(" {}: choose a direction ", a.name()),
        None => format!(" delve · floor {} ", game.depth),
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(title)
        .title_style(Style::new().add_modifier(Modifier::BOLD));
    let inner = block.inner(map_area);
    f.render_widget(block, map_area);
    draw_map(f.buffer_mut(), inner, game);

    draw_side(f, side, game);
    draw_log(f, log, game);

    match game.state {
        State::Playing => {}
        State::Dead => draw_end(f, game, "You died", Color::Red),
        State::Escaped => draw_end(f, game, "Escaped!", Color::Green),
        State::Won => draw_end(f, game, "Victory!", GOLD),
    }
}

/// Top-left map cell shown, so that the hero stays centered on small terminals.
fn camera(view: u16, len: i32, hero: i32) -> i32 {
    let view = view as i32;
    if view >= len { 0 } else { (hero - view / 2).clamp(0, len - view) }
}

fn draw_map(buf: &mut Buffer, area: Rect, game: &Game) {
    let cam_x = camera(area.width, map::WIDTH, game.player.x);
    let cam_y = camera(area.height, map::HEIGHT, game.player.y);
    for sy in 0..area.height.min(map::HEIGHT as u16) {
        for sx in 0..area.width.min(map::WIDTH as u16) {
            let p = Pos::new(cam_x + sx as i32, cam_y + sy as i32);
            if let Some((ch, color)) = glyph_at(game, p)
                && let Some(cell) = buf.cell_mut((area.x + sx, area.y + sy))
            {
                cell.set_char(ch).set_fg(color);
                // The cells of coming blows, on a red ground.
                if game.map.is_visible(p) && game.threatened(p) {
                    cell.set_bg(THREAT);
                }
            }
        }
    }
}

fn glyph_at(game: &Game, p: Pos) -> Option<(char, Color)> {
    let visible = game.map.is_visible(p);
    if !visible && !game.map.is_explored(p) {
        return None;
    }
    if p == game.player {
        return Some(('@', HERO));
    }
    if visible {
        if let Some(i) = game.monster_at(p) {
            return Some((game.monsters[i].kind.glyph, monster_color(game.monsters[i].kind.glyph)));
        }
        if let Some(i) = game.item_at(p) {
            let kind = game.items[i].kind;
            let color = match kind.glyph() {
                '!' => POTION,
                '%' => OIL,
                _ => GOLD,
            };
            return Some((kind.glyph(), color));
        }
    }
    Some(match (game.map.tile(p), visible) {
        (Tile::Wall, true) => ('#', WALL),
        (Tile::Wall, false) => ('#', WALL_SEEN),
        (Tile::Floor, true) => ('·', FLOOR),
        (Tile::Floor, false) => ('·', FLOOR_SEEN),
        (Tile::StairsDown, true) => ('>', Color::White),
        (Tile::StairsDown, false) => ('>', FLOOR),
        (Tile::Exit, true) => ('<', Color::Green),
        (Tile::Exit, false) => ('<', FLOOR),
    })
}

fn monster_color(glyph: char) -> Color {
    match glyph {
        'r' => Color::Rgb(170, 130, 90),
        'k' => Color::Rgb(120, 180, 90),
        'g' => Color::Rgb(80, 200, 80),
        'o' => Color::Rgb(220, 120, 60),
        's' => Color::Rgb(160, 100, 220),
        'T' => Color::Rgb(90, 170, 170),
        'W' => Color::Rgb(200, 200, 255),
        'D' => Color::Rgb(255, 60, 40),
        'S' => Color::Rgb(150, 110, 200),
        _ => Color::White,
    }
}

fn draw_side(f: &mut Frame, area: Rect, game: &Game) {
    let block = Block::bordered().border_type(BorderType::Rounded).title(" hero ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [hp, xp, torch, stats] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1), Constraint::Min(0)])
            .areas(inner);

    let s = game.stats;
    let ratio = (s.hp as f64 / s.max_hp as f64).clamp(0.0, 1.0);
    let hp_color = if ratio > 0.5 {
        Color::Green
    } else if ratio > 0.25 {
        Color::Yellow
    } else {
        Color::Red
    };
    f.render_widget(
        Gauge::default()
            .ratio(ratio)
            .label(format!("HP {}/{}", s.hp, s.max_hp))
            .gauge_style(Style::new().fg(hp_color).bg(Color::Rgb(40, 40, 40))),
        hp,
    );
    let xp_ratio = (game.xp as f64 / game.xp_to_next() as f64).clamp(0.0, 1.0);
    f.render_widget(
        Gauge::default()
            .ratio(xp_ratio)
            .label(format!("Lv {} · XP {}/{}", game.level, game.xp, game.xp_to_next()))
            .gauge_style(Style::new().fg(Color::Blue).bg(Color::Rgb(40, 40, 40))),
        xp,
    );
    let label = if game.torch > 0 { format!("Torch {}/{TORCH_MAX}", game.torch) } else { "Torch out!".into() };
    f.render_widget(
        Gauge::default()
            .ratio((game.torch as f64 / TORCH_MAX as f64).clamp(0.0, 1.0))
            .label(label)
            .gauge_style(Style::new().fg(OIL).bg(Color::Rgb(40, 40, 40))),
        torch,
    );

    let dim = Style::new().fg(Color::DarkGray);
    let key = Style::new().fg(Color::Cyan);
    let mut lines = vec![
        Line::raw(""),
        Line::from(format!("Attack  {}", s.attack)),
        Line::from(format!("Defense {}", s.defense)),
        Line::from(vec![Span::raw("Potions "), Span::styled(game.potions.to_string(), Style::new().fg(POTION))]),
        Line::from(vec![Span::raw("Gold    "), Span::styled(game.gold.to_string(), Style::new().fg(GOLD))]),
        Line::from(format!("Kills   {}", game.kills)),
        Line::styled(format!("Turn {} · seed {}", game.turn, game.seed), dim),
        Line::raw(""),
    ];
    for (key, ability) in ['d', 'f', 't'].into_iter().zip(Ability::ALL) {
        let cd = game.cooldown(ability);
        let state = if cd > 0 {
            format!("in {cd}")
        } else if ability == Ability::Throw && game.torch < THROW_COST {
            "no light".into()
        } else {
            "ready".into()
        };
        let style = if game.can_use(ability).is_ok() { Style::new().fg(Color::Yellow) } else { dim };
        lines.push(Line::from(vec![
            Span::styled(format!("{key} {:<12}", ability.name()), style),
            Span::styled(state, dim),
        ]));
    }
    lines.push(Line::raw(""));
    for (k, what) in [
        ("arrows hjkl", "move"),
        ("yubn", "diagonals"),
        ("bump", "attack"),
        (". space", "wait"),
        (">", "go down"),
        ("<", "leave (floors 3 6 9)"),
        ("p", "drink potion"),
        ("d f t +dir", "abilities"),
        ("red ground", "blows to come"),
        ("q", "quit"),
    ] {
        lines.push(Line::from(vec![Span::styled(format!("{k:<12}"), key), Span::styled(what, dim)]));
    }
    f.render_widget(Paragraph::new(lines), stats);
}

fn draw_log(f: &mut Frame, area: Rect, game: &Game) {
    let block = Block::bordered().border_type(BorderType::Rounded).title(" log ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let n = inner.height as usize;
    let start = game.log.len().saturating_sub(n);
    let last = game.log.len().saturating_sub(1);
    let lines: Vec<Line> = game.log[start..]
        .iter()
        .enumerate()
        .map(|(i, (msg, tone))| {
            let color = match tone {
                Tone::Normal => Color::Gray,
                Tone::Good => Color::Green,
                Tone::Bad => Color::Red,
            };
            // Older messages fade out.
            let style = if start + i == last {
                Style::new().fg(color)
            } else {
                Style::new().fg(color).add_modifier(Modifier::DIM)
            };
            Line::styled(msg.as_str(), style)
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_end(f: &mut Frame, game: &Game, title: &str, color: Color) {
    let area = f.area();
    let (w, h) = (46.min(area.width), 10.min(area.height));
    let popup = Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h);
    let cause = match (game.state, game.killed_by.as_deref()) {
        (State::Won, _) => format!("You escaped with the Amulet from floor {}.", game.depth),
        (State::Escaped, _) => format!("You climbed out from floor {}.", game.depth),
        (_, Some(name)) => format!("Killed by a {name} on floor {}.", game.depth),
        _ => format!("Died on floor {}.", game.depth),
    };
    let score = game.score();
    let text = vec![
        Line::raw(""),
        Line::raw(cause),
        Line::raw(format!("Level {} · {} kills", game.level, game.kills)),
        if game.gold_is_safe() {
            Line::styled(format!("+{} gold to your purse", game.gold), Style::new().fg(GOLD))
        } else {
            Line::styled(format!("{} gold lost in the dark", game.gold), Style::new().fg(Color::Red))
        },
        Line::styled(format!("Score {score}"), Style::new().add_modifier(Modifier::BOLD)),
        Line::raw(""),
        Line::styled("r: new dungeon   q: quit", Style::new().fg(Color::Cyan)),
    ];
    f.render_widget(Clear, popup);
    f.render_widget(
        Paragraph::new(text).centered().wrap(Wrap { trim: true }).block(
            Block::bordered()
                .border_type(BorderType::Double)
                .border_style(Style::new().fg(color))
                .title(format!(" {title} "))
                .title_style(Style::new().fg(color).add_modifier(Modifier::BOLD)),
        ),
        popup,
    );
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    fn render(game: &Game, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| draw(f, game, None)).unwrap();
        let buf = terminal.backend().buffer();
        (0..h).map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>() + "\n").collect()
    }

    #[test]
    fn hero_is_drawn_even_on_small_terminals() {
        let game = Game::new(1);
        for (w, h) in [(120, 50), (60, 24)] {
            let screen = render(&game, w, h);
            assert!(screen.contains('@'), "{w}x{h}:\n{screen}");
            assert!(screen.contains("floor 1"));
        }
    }

    /// `cargo test print_screen -- --ignored --nocapture` shows a frame.
    #[test]
    #[ignore]
    fn print_screen() {
        print!("{}", render(&Game::new(1), 110, 50));
    }
}
