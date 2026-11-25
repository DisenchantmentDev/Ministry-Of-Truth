# Ministry of Truth

Ministry of Truth is a desktop notebook for tracking lore during Minecraft roleplay events, allowing staff members to keep notes on nations, players, and major events in one centralized project, designed with the complexities of these events in mind.

The app is written in Rust with [egui](https://github.com/emilk/egui) and [eframe](https://github.com/emilk/egui/tree/master/crates/eframe).

## What it is

Roleplay events produce a lot of connected information. This information, as we have found in Ish's state, can be a nightmare to track, and finding a clean solution that prevents information from being lost, but also standardizes the format, has been nearly impossible. Thus, Ministry of Truth was born. 

Ministry of Truth stores each piece of information as a page. Pages hold structured fields (ex. which nation a player belongs to or what day an event happened) as well as a Markdown formatted field for extra notes to be written by the staff member. All information in the structured fields are used to create a structured tree of information, and can also be used to populate a timeline.

## Current features

### Projects

- A project is a folder. Creating a project writes a `project.json` file with the project name and an optional Day 0 date.
- Pages are plain Markdown files with a YAML header. The app reads and writes these files directly, so a project can be copied, backed up, or put under version control.
- The project tree on the left shows folders and pages. Folders and pages can be created, renamed, and deleted.
- The app warns about pages that share a title.

### Page types

| Type   | Structured fields                                                        |
|--------|--------------------------------------------------------------------------|
| Note   | Title and Markdown body only                                             |
| Nation | Leaders, location, founded day, and relations to other nations           |
| Player | Nation, Discord name, and death day                                      |
| Event  | Day, event type, location, nations, and participants                     |

Relations between nations use one of four stances: Allied, Hostile, Neutral, or Vassal.

Event types are Assassination, War, Battle, Diplomacy, and Other.

Templates are available for blank notes, assassinations, wars, players, and nations. Plain events are created from a nation's Events tab.

A nation page has four tabs:

- **Notes** holds the Markdown body.
- **Events** lists every event that names the nation, and creates a new linked event.
- **Relations** edits the stance toward other nations.
- **Map** lists each map with the number of chunks the nation holds on it, and opens the map.

A nation also has a **Map color**. If none is set, the app picks a stable color from the page's id.

### Links

- Wiki links use the form `[[Title]]`. A link resolves by page title or by file name.
- Clicking a resolved link opens the page. Unresolved links are marked "(missing)" in the preview, and a button creates a note with that title.
- Renaming a page updates the wiki links, leader lists, nation fields, and event fields that refer to the old title in other pages.

### Editing

- Each page can be viewed in four modes: Edit (source only), Preview (rendered Markdown), Split (source and preview side by side), and Live (styled source editing).
- Pages can open in floating windows that can be moved and resized. This lets two pages stay visible at once.
- Save with `Ctrl+S` or from the File menu. Switching pages and closing a project also save the open page.

### Timeline

- The timeline lists events, nation foundings on or after Day 0, and recorded player deaths.
- Entries are grouped by day. If a Day 0 date is set, each day shows its calendar date.
- Events without a day appear at the end under "Undated".
- Nations founded before Day 0 are left off the timeline.

### Maps and borders

- A map is an image of the game world, such as a PNG exported from Xaero's World Map. Choose **Import** under Maps in the project tree, pick the image, and set where it sits in the world.
- Xaero's images carry no coordinates, so each map has a calibration: pixels per block and the block position of the top-left corner. The Calibration panel can compute these from one or two reference pixels whose block coordinates you know.
- Territory is drawn per chunk (16 by 16 blocks). Select a nation in the legend, then paint chunks with the Brush, Rectangle, or Fill tool. The Eraser clears chunks, and Pick selects the nation under the cursor. Each chunk has at most one owner.
- Claimed chunks show as a tinted shape in the nation's color with an outline. The legend shows each nation's chunk count and area.
- Pan by dragging with the middle or right mouse button (or hold Space), and zoom with the scroll wheel. Undo and redo with `Ctrl+Z` and `Ctrl+Shift+Z`. Tool keys are B, R, F, E, and I.
- Maps save with `Ctrl+S`, when you switch views, and when you close the project.
- Each map is a folder under `maps/` with `map.json` (calibration), the image, and `claims.json` (chunks per nation, stored as runs). Claims refer to nation pages by id, so renaming a nation keeps its territory.
- Importing Xaero's world map folder (the `.xwmc` region files) is not supported yet. Export an image from Xaero's World Map instead.

### Platforms

- The native desktop app is the main target. It handles folder selection and file access.
- A web build is set up with Trunk. The web build can display the interface, but it cannot open or create project folders.

## Planned features

These features are not yet implemented.

### Map extras

- **Importing Xaero's world map folder** directly, without exporting an image first.
- **Events and locations on the map**, placed at block coordinates.
- **Border history** that changes with the timeline, so territory can be shown on a given day.

### Ticket import and tracking

- **Ticket import** would bring in requests, reports, or issues externally from ticket bots in discord to help track player requests and lore as an event is happening. This would allow for intake of raw JSON files and to link them to individual players already logged in MoT.
- **Ticket tracking** would keep a list of open items and their status, and link each item to the nations, players, or events it concerns.

## Project layout

A new project has this structure:

```text
my-project/
├── project.json
├── index.md
├── nations/
│   └── index.md
├── players/
│   └── index.md
└── events/
    └── index.md
```

Nation, player, and event pages are filed in `nations/`, `players/`, and `events/`. Notes are filed in the folder selected in the tree.

## Building and running

Requirements:

- Rust 1.92 or newer.
- On Debian or Ubuntu, the packages needed by eframe:

  ```sh
  sudo apt-get install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev
  ```

Run the desktop app:

```sh
cargo run --release
```

Run the tests:

```sh
cargo test
```

### Web build

The web build uses [Trunk](https://trunkrs.dev/) and the `wasm32-unknown-unknown` target.

1. Install the target with `rustup target add wasm32-unknown-unknown`.
2. Install Trunk with `cargo install --locked trunk`.
3. Run `trunk serve` and open `http://127.0.0.1:8080`.

Run `trunk build --release` to produce a static site in `dist`.
