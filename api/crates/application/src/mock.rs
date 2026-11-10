//! Mock server planning: turns a collection's mock responses into routes,
//! and starts / persists mock servers on the services runtime.

use crate::AppServices;
use crate::send::variables_for;
use ferrumapi_domain::mock::{CollectionMocks, MockLogEntry, MockRoute, route_path};
use ferrumapi_domain::{Collection, CollectionId, Environment, RequestOrigin};
use ferrumapi_persistence::StoreResult;
use ferrumapi_server::{MockServer, ServerError};
use tokio::sync::mpsc::UnboundedSender;

/// Routes for every enabled mock response of `collection`, in tree order
/// (then list order). Paths use each request's variables (globals <
/// collection < environment); mocks of deleted requests are ignored.
#[must_use]
pub fn plan_mock_routes(
    collection: &Collection,
    mocks: &CollectionMocks,
    globals: &Environment,
    environment: Option<&Environment>,
) -> Vec<MockRoute> {
    let collections = std::slice::from_ref(collection);
    let mut routes = Vec::new();
    for saved in collection.requests() {
        let origin = RequestOrigin {
            collection: collection.id,
            item: saved.id,
        };
        let vars = variables_for(Some(origin), collections, globals, environment);
        let path = route_path(&saved.request.url, &vars);
        routes.extend(
            mocks
                .for_item(saved.id)
                .filter(|m| m.enabled)
                .map(|m| MockRoute {
                    name: saved.name.clone(),
                    method: saved.request.method,
                    path: path.clone(),
                    response: m.clone(),
                }),
        );
    }
    routes
}

impl AppServices {
    /// Starts a mock server on `127.0.0.1:port` (must run on the services runtime).
    pub async fn start_mock_server(
        &self,
        port: u16,
        routes: Vec<MockRoute>,
        log: UnboundedSender<MockLogEntry>,
    ) -> Result<MockServer, ServerError> {
        MockServer::start(port, routes, log).await
    }

    pub async fn load_mocks(&self, collection: CollectionId) -> StoreResult<CollectionMocks> {
        self.store.load_mocks(collection).await
    }

    pub async fn save_mocks(
        &self,
        collection: CollectionId,
        mocks: CollectionMocks,
    ) -> StoreResult<()> {
        self.store.save_mocks(collection, &mocks).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumapi_domain::mock::MockResponse;
    use ferrumapi_domain::{Folder, HttpMethod, HttpRequest, Item, SavedRequest, Variable};

    #[test]
    fn routes_follow_tree_order_and_skip_disabled_or_orphaned() {
        let users = SavedRequest::new(
            "List users",
            HttpRequest::new(HttpMethod::Get, "{{baseUrl}}/users"),
        );
        let user = SavedRequest::new(
            "Get user",
            HttpRequest::new(HttpMethod::Get, "{{host}}/{{ver}}/users/:id"),
        );
        let mut folder = Folder::new("Nested");
        folder.items.push(Item::Request(user.clone()));
        let mut c = Collection::new("API");
        c.variables = vec![
            Variable::new("ver", "v2"),
            Variable::new("host", "http://x"),
        ];
        c.items = vec![Item::Folder(folder), Item::Request(users.clone())];

        let mut off = MockResponse::for_item(users.id);
        off.enabled = false;
        let mocks = CollectionMocks {
            port: 4010,
            responses: vec![
                MockResponse::for_item(users.id),
                off,
                MockResponse::for_item(user.id),
                MockResponse::for_item(ferrumapi_domain::ItemId::new()),
            ],
        };
        let routes = plan_mock_routes(&c, &mocks, &Environment::globals(), None);
        let got: Vec<(&str, &str)> = routes
            .iter()
            .map(|r| (r.name.as_str(), r.path.as_str()))
            .collect();
        assert_eq!(
            got,
            [("Get user", "/v2/users/:id"), ("List users", "/users")]
        );
        assert_eq!(routes[0].method, HttpMethod::Get);
    }
}
