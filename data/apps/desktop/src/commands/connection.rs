//! Profile reload/delete and connect/disconnect. Saving lives in the
//! connection form.

use super::{background, log_dropped};
use crate::app::FerrumData;
use crate::state::ConnStatus;
use ferrum_ui::prelude::*;
use ferrumdata_domain::ProfileId;

impl FerrumData {
    pub fn reload_profiles(&mut self, cx: &mut Context<Self>) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            if let Ok(Ok(profiles)) =
                background(
                    &services,
                    async move { svc.connections.list_profiles().await },
                )
                .await
            {
                log_dropped(this.update(cx, |this, cx| {
                    this.state.profiles = profiles;
                    cx.notify();
                }));
            }
        })
        .detach();
    }

    pub fn delete_profile(&mut self, id: ProfileId, cx: &mut Context<Self>) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let result = background(&services, async move {
                svc.connections.delete_profile(id).await
            })
            .await;
            log_dropped(this.update(cx, |this, cx| match result {
                Ok(Ok(())) => {
                    this.state.conn_status.remove(&id);
                    for tab in this
                        .state
                        .tabs
                        .iter_mut()
                        .filter(|t| t.profile_id == Some(id))
                    {
                        tab.profile_id = None;
                        tab.unsaved = true;
                    }
                    // Its query history was deleted with it.
                    this.invalidate_history();
                    this.reload_profiles(cx);
                }
                Ok(Err(e)) => this.notify_user(e.message, true, cx),
                Err(e) => this.notify_user(e, true, cx),
            }));
        })
        .detach();
    }

    pub fn connect(&mut self, id: ProfileId, cx: &mut Context<Self>) {
        self.state.conn_status.insert(id, ConnStatus::Connecting);
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let result =
                background(&services, async move { svc.connections.connect(id).await }).await;
            let status = match result {
                Ok(Ok(info)) => ConnStatus::Ready(info.version),
                Ok(Err(e)) => ConnStatus::Failed(e.message),
                Err(e) => ConnStatus::Failed(e),
            };
            log_dropped(this.update(cx, |this, cx| {
                this.state.conn_status.insert(id, status);
                cx.notify();
            }));
        })
        .detach();
    }

    pub fn disconnect(&mut self, id: ProfileId, cx: &mut Context<Self>) {
        self.state.conn_status.remove(&id);
        let svc = self.services.clone();
        // Fire-and-forget: the pool closes in the background.
        drop(
            self.services
                .spawn(async move { svc.connections.disconnect(id).await }),
        );
        cx.notify();
    }
}
