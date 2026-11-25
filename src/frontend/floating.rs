use crate::backend::Project;
use crate::frontend::editor;
use crate::frontend::session::FloatingPage;
use crate::frontend::UiAction;

/// Draw every floating page window. Closed windows emit [`UiAction::CloseFloating`].
pub(crate) fn show(ui: &mut egui::Ui, project: &Project, floating: &mut [FloatingPage]) -> Vec<UiAction> {
    let mut actions = Vec::new();
    for page in floating.iter_mut() {
        if !page.visible {
            continue;
        }
        let title = egui::RichText::new(page.open.frontmatter.title.clone())
            .text_style(egui::TextStyle::Button);
        let id = page.open.id;
        let mut open = true;
        // Heading is 30px, which makes the stock title bar bulky. A button-sized
        // title plus a short vertical margin keeps the bar to the interact-size floor.
        let frame = egui::Frame::window(ui.style()).inner_margin(egui::Margin::symmetric(8, 2));
        egui::Window::new(title)
            .id(egui::Id::new(("floating_page", id)))
            .frame(frame)
            .open(&mut open)
            .resizable(true)
            .default_size([520.0, 420.0])
            .min_width(280.0)
            .min_height(200.0)
            .show(ui.ctx(), |ui| {
                let action = editor::show(
                    ui,
                    project,
                    &mut page.open,
                    &mut page.mode,
                    &mut page.nation_tab,
                    &mut page.cache,
                    &mut page.live_markdown,
                );
                if !matches!(action, UiAction::None) {
                    actions.push(action);
                }
            });
        if !open {
            page.visible = false;
            actions.push(UiAction::CloseFloating(id));
        }
    }
    actions
}
