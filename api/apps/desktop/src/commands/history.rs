//! History list refresh and deletion.

use crate::app::FerrumApi;
use ferrumapi_domain::HistoryId;
use gpui::Context;

impl FerrumApi {
    pub fn refresh_history(&mut self, cx: &mut Context<Self>) {
        let limit = self.history_limit();
        let svc = self.services.clone();
        self.io(
            cx,
            async move { svc.store.list_history(limit).await },
            |this, result, _| match result {
                Ok(list) => this.state.history = list,
                Err(e) => tracing::warn!(error = %e, "failed to load history"),
            },
        );
    }

    pub fn delete_history(&mut self, id: HistoryId, cx: &mut Context<Self>) {
        self.state.history.retain(|h| h.id != id);
        let svc = self.services.clone();
        self.io(
            cx,
            async move { svc.store.delete_history(id).await },
            Self::report("Could not delete history entry"),
        );
    }

    pub fn clear_history(&mut self, cx: &mut Context<Self>) {
        self.state.history.clear();
        let svc = self.services.clone();
        self.io(
            cx,
            async move { svc.store.clear_history().await },
            Self::report("Could not clear history"),
        );
    }
}
