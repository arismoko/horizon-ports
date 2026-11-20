// ChatScreenMixin.sendMessage: a message holding the shared item's name is announced to the server
// as `smwyg:share`, then sent as ordinary chat (`send` always lets the game go on).
fn share_send(game: &mut Game<'_>, message: &str) -> core::ops::ControlFlow<()> {
    if crate::game::smwyg::send(game, message) {
        return core::ops::ControlFlow::Break(());
    }
    core::ops::ControlFlow::Continue(())
}
