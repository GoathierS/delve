//! delve in the terminal. Shares its saves with the graphical version.

mod ui;

use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use delve_core::game::{Ability, Action, Game, State, Tone};
use delve_core::save::Storage;

const USAGE: &str = "usage: delve-term [--seed N]

A saved game is resumed unless --seed asks for a new dungeon.";

fn main() -> io::Result<()> {
    let mut seed = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seed" => match args.next().and_then(|s| s.parse().ok()) {
                Some(n) => seed = Some(n),
                None => return Err(io::Error::other(USAGE)),
            },
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ => return Err(io::Error::other(USAGE)),
        }
    }
    let storage = Storage::default_location();
    let game = match seed {
        Some(seed) => new_run(&storage, seed),
        None => resume_or_start(&storage),
    };
    let terminal = ratatui::init();
    let result = run(terminal, game, &storage);
    ratatui::restore();
    result
}

fn random_seed() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0) % 1_000_000
}

/// A new hero, carrying the equipment bought in the graphical version's armory.
fn new_run(storage: &Storage, seed: u64) -> Game {
    Game::equipped(seed, &storage.profile().loadout())
}

fn resume_or_start(storage: &Storage) -> Game {
    match storage.load() {
        Ok(Some(mut game)) => {
            game.say("Welcome back.", Tone::Normal);
            game
        }
        Ok(None) => new_run(storage, random_seed()),
        Err(e) => {
            let mut game = new_run(storage, random_seed());
            game.say(format!("A new dungeon: {e}."), Tone::Bad);
            game
        }
    }
}

fn run(mut terminal: DefaultTerminal, mut game: Game, storage: &Storage) -> io::Result<()> {
    // Leaving mid-run saves it; the run is entered in the scores once, when it ends.
    let quit = |game: &Game| if game.state == State::Playing { storage.save(game) } else { Ok(()) };
    let mut finished = false;
    // The ability waiting for a direction, if any.
    let mut aim: Option<Ability> = None;
    loop {
        terminal.draw(|f| ui::draw(f, &game, aim))?;
        let Event::Key(key) = event::read()? else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return quit(&game);
        }
        if let Some(ability) = aim.take() {
            // The next direction fires the ability; anything else puts it away.
            if let Some(Action::Move(dx, dy)) = action_for(key.code) {
                game.act(Action::Use(ability, dx, dy));
            }
            continue;
        }
        match (game.state, key.code) {
            (_, KeyCode::Char('q') | KeyCode::Esc) => return quit(&game),
            (State::Playing, KeyCode::Char(c @ ('d' | 'f' | 't'))) => {
                let ability = match c {
                    'd' => Ability::Dash,
                    'f' => Ability::Bash,
                    _ => Ability::Throw,
                };
                match game.can_use(ability) {
                    Ok(()) => aim = Some(ability),
                    Err(why) => game.say(why, Tone::Normal),
                }
            }
            (State::Dead | State::Escaped | State::Won, KeyCode::Char('r')) => {
                game = new_run(storage, random_seed());
                finished = false;
            }
            (State::Playing, code) => {
                if let Some(action) = action_for(code) {
                    let depth = game.depth;
                    game.act(action);
                    if game.depth != depth {
                        // Autosave on each new floor, so a crash costs at most one floor.
                        storage.save(&game)?;
                    }
                }
            }
            _ => {}
        }
        if game.state != State::Playing && !finished {
            finished = true;
            storage.finish(&game)?;
        }
    }
}

fn action_for(code: KeyCode) -> Option<Action> {
    Some(match code {
        KeyCode::Left | KeyCode::Char('h' | '4') => Action::Move(-1, 0),
        KeyCode::Right | KeyCode::Char('l' | '6') => Action::Move(1, 0),
        KeyCode::Up | KeyCode::Char('k' | '8') => Action::Move(0, -1),
        KeyCode::Down | KeyCode::Char('j' | '2') => Action::Move(0, 1),
        KeyCode::Char('y' | '7') => Action::Move(-1, -1),
        KeyCode::Char('u' | '9') => Action::Move(1, -1),
        KeyCode::Char('b' | '1') => Action::Move(-1, 1),
        KeyCode::Char('n' | '3') => Action::Move(1, 1),
        KeyCode::Char('.' | ' ' | '5') => Action::Wait,
        KeyCode::Char('>') => Action::Descend,
        KeyCode::Char('<') => Action::Escape,
        KeyCode::Char('p') => Action::Drink,
        _ => return None,
    })
}
