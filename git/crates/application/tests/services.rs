//! Service-level tests: profiles drive identity and SSH environment per repo.

use ferrumgit_application::{AppServices, PassphraseChange};
use ferrumgit_credentials::SessionStore;
use ferrumgit_domain::{Profile, RepoOp, Secret, SshAuth};
use ferrumgit_git::Git;
use std::sync::Arc;

fn services(dir: &std::path::Path) -> AppServices {
    let cfg = dir.join("gitconfig");
    std::fs::write(
        &cfg,
        "[user]\n\tname = Global\n\temail = global@example.com\n[commit]\n\tgpgsign = false\n",
    )
    .expect("cfg");
    let git = Git::new()
        .with_env("GIT_CONFIG_GLOBAL", cfg.as_os_str())
        .with_env("GIT_CONFIG_NOSYSTEM", "1")
        .with_askpass("/usr/bin/false");
    AppServices::start_with(&dir.join("data"), git, Arc::new(SessionStore::default()))
        .expect("services")
}

#[test]
fn per_repo_profiles_set_identity_and_ssh_key() {
    let tmp = tempfile::tempdir().expect("tmp");
    let svc = services(tmp.path());
    let root = tmp.path().to_path_buf();
    let s = svc.clone();
    svc.spawn_blocking_test(async move {
        let key = root.join("id_work");
        std::fs::write(&key, "not a real key").expect("key");

        let mut work = Profile::new("Work");
        work.author_name = "Work Me".into();
        work.author_email = "me@work.example".into();
        work.ssh = SshAuth::KeyFile {
            path: key.display().to_string(),
            has_passphrase: false,
        };
        let work = s
            .save_profile(work, PassphraseChange::Set(Secret::new("pw")))
            .await
            .expect("save");
        assert!(matches!(
            work.ssh,
            SshAuth::KeyFile {
                has_passphrase: true,
                ..
            }
        ));

        // Two repos, one assigned to the Work profile, one using global config.
        let a = s
            .init_repo(&root.join("a").display().to_string())
            .await
            .expect("init a");
        let a = s.set_repo_profile(a, Some(work.id)).await.expect("assign");
        let b = s
            .init_repo(&root.join("b").display().to_string())
            .await
            .expect("init b");
        for (entry, profile) in [(&a, Some(&work)), (&b, None)] {
            std::fs::write(std::path::Path::new(&entry.path).join("f"), "x").expect("write");
            s.run_op(entry, profile, &RepoOp::StageAll, None)
                .await
                .expect("stage");
            s.run_op(
                entry,
                profile,
                &RepoOp::Commit {
                    message: "c".into(),
                    amend: false,
                },
                None,
            )
            .await
            .expect("commit");
        }
        let author = |p: String| {
            let s = s.clone();
            async move {
                s.load(&p, 10).await.expect("load").snapshot.commits[0]
                    .author_email
                    .clone()
            }
        };
        assert_eq!(author(a.path.clone()).await, "me@work.example");
        assert_eq!(author(b.path.clone()).await, "global@example.com");

        // The work profile's network env pins its key and carries the passphrase file.
        let env = s.net_env(Some(&work)).await.expect("env");
        let vars: Vec<(String, String)> = env
            .vars()
            .iter()
            .map(|(k, v)| (k.clone(), v.to_string_lossy().into_owned()))
            .collect();
        let ssh = &vars
            .iter()
            .find(|(k, _)| k == "GIT_SSH_COMMAND")
            .expect("ssh cmd")
            .1;
        assert!(
            ssh.contains("id_work") && ssh.contains("IdentitiesOnly=yes"),
            "{ssh}"
        );
        assert!(
            vars.iter()
                .any(|(k, _)| k == ferrumgit_git::ASKPASS_FILE_ENV)
        );
        let system = s.net_env(None).await.expect("env");
        assert!(!system.vars().iter().any(|(k, _)| k == "GIT_SSH_COMMAND"));

        // Repo list remembers the assignment; deleting the profile clears it.
        let listed = s.store.list_repos().await.expect("repos");
        assert_eq!(
            listed.iter().find(|r| r.id == a.id).expect("a").profile,
            Some(work.id)
        );
        s.delete_profile(work.id).await.expect("delete");
        let listed = s.store.list_repos().await.expect("repos");
        assert_eq!(
            listed.iter().find(|r| r.id == a.id).expect("a").profile,
            None
        );

        // Re-adding a repo by a subdirectory resolves to the same entry.
        std::fs::create_dir_all(root.join("b/sub")).expect("sub");
        let again = s
            .add_repo(&root.join("b/sub").display().to_string())
            .await
            .expect("re-add");
        assert_eq!(again.id, b.id);
        assert!(
            s.add_repo(&root.display().to_string()).await.is_err(),
            "not a repository"
        );
    });
}

#[test]
fn clone_assigns_profile() {
    let tmp = tempfile::tempdir().expect("tmp");
    let svc = services(tmp.path());
    let root = tmp.path().to_path_buf();
    let s = svc.clone();
    svc.spawn_blocking_test(async move {
        let src = s
            .init_repo(&root.join("src").display().to_string())
            .await
            .expect("src");
        std::fs::write(root.join("src/f"), "x").expect("w");
        s.run_op(&src, None, &RepoOp::StageAll, None)
            .await
            .expect("stage");
        s.run_op(
            &src,
            None,
            &RepoOp::Commit {
                message: "c".into(),
                amend: false,
            },
            None,
        )
        .await
        .expect("commit");

        let p = s
            .save_profile(Profile::new("Personal"), PassphraseChange::Keep)
            .await
            .expect("p");
        let dest = root.join("dst").display().to_string();
        let cloned = s
            .clone_repo(&src.path, &dest, Some(p.clone()), None)
            .await
            .expect("clone");
        assert_eq!(cloned.profile, Some(p.id));
        let data = s.load(&cloned.path, 10).await.expect("load");
        assert_eq!(data.graph.len(), 1);
        assert_eq!(data.snapshot.remotes[0].name, "origin");
        assert!(data.snapshot.refs.iter().any(|r| r.name == "origin/main"));
    });
}

trait BlockOn {
    fn spawn_blocking_test<F: std::future::Future<Output = ()> + Send + 'static>(&self, f: F);
}

impl BlockOn for AppServices {
    fn spawn_blocking_test<F: std::future::Future<Output = ()> + Send + 'static>(&self, f: F) {
        let handle = self.spawn(f);
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("rt");
        if let Err(e) = rt.block_on(handle) {
            std::panic::resume_unwind(e.into_panic());
        }
    }
}
