//! 确认后的 UID 范围快照：一个 WAL 读事务、keyset 页、每页立即落既有 jobs。
use crate::{
    Error, Result,
    store::{
        organization::{self, PhotoRef},
        query::{self, Query, Scope},
    },
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Identity {
    pub repository_id: String,
    pub asset_id: i64,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Range {
    Selected {
        photos: Vec<Identity>,
    },
    Directory {
        #[serde(rename = "repositoryId")]
        repository_id: String,
        directory: String,
        recursive: bool,
    },
    Repository {
        #[serde(rename = "repositoryId")]
        repository_id: String,
    },
}
impl Range {
    pub fn repositories(&self) -> Result<Vec<String>> {
        let ids: std::collections::BTreeSet<_> = match self {
            Self::Selected { photos } => {
                if photos.is_empty()
                    || photos.len() > 100_000
                    || photos.iter().any(|p| p.asset_id <= 0)
                {
                    return Err(Error::Unsupported("请选择 1–100000 张照片".into()));
                }
                photos.iter().map(|p| p.repository_id.clone()).collect()
            }
            Self::Directory {
                repository_id,
                directory,
                ..
            } => {
                if directory.len() > 32_768
                    || directory
                        .replace('\\', "/")
                        .split('/')
                        .any(|p| p == ".." || p == ".")
                {
                    return Err(Error::Unsupported("识别目录无效".into()));
                }
                [repository_id.clone()].into()
            }
            Self::Repository { repository_id } => [repository_id.clone()].into(),
        };
        if ids.iter().any(|id| id.is_empty() || id.len() > 128) {
            return Err(Error::Unsupported("识别库身份无效".into()));
        }
        Ok(ids.into_iter().collect())
    }
}
pub fn freeze(
    conn: &Connection,
    repo: &str,
    range: &Range,
    mut sink: impl FnMut(Vec<PhotoRef>) -> Result<()>,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    if let Range::Selected { photos } = range {
        let ids = photos
            .iter()
            .filter(|p| p.repository_id == repo)
            .map(|p| p.asset_id)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        for ids in ids.chunks(super::jobs::MAX_PAGE) {
            sink(organization::photo_refs(&tx, repo, ids)?)?;
        }
    } else {
        let scope = match range {
            Range::Directory {
                directory,
                recursive,
                ..
            } => {
                if *recursive {
                    Scope::subtree(directory)
                } else {
                    Scope::Directory {
                        rel_path: directory.clone(),
                    }
                }
            }
            _ => Scope::Repository,
        };
        let query = Query::new(scope);
        let upper = query::identity_upper_bound(&tx)?;
        let mut after = 0;
        loop {
            let rows = query::identity_page(&tx, &query, after, upper, super::jobs::MAX_PAGE)?;
            let Some(last) = rows.last() else {
                break;
            };
            after = last.0;
            sink(
                rows.into_iter()
                    .map(|(asset_id, photo_uid)| PhotoRef {
                        repository_id: repo.into(),
                        photo_uid,
                        asset_id,
                    })
                    .collect(),
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direct_directory_is_bounded_and_raw_is_transparent_without_inheriting_filters() {
        let mut c = Connection::open_in_memory().unwrap();
        crate::store::migration::apply(
            &mut c,
            crate::store::migration::DbKind::Catalog,
            crate::store::migration::Backups::none(),
            1,
        )
        .unwrap();
        for (id, path) in [
            (1, "photos/旅行/a.jpg"),
            (2, "photos/旅行/_RAW/b.nef"),
            (3, "photos/旅行/下级/c.jpg"),
            (4, "photos/旅行_别的/d.jpg"),
        ] {
            c.execute("INSERT INTO assets(id,organization_uid,rating,imported_at,updated_at) VALUES(?1,?2,0,1,1)",rusqlite::params![id,format!("uid{id}")]).unwrap();
            c.execute("INSERT INTO asset_files(asset_id,rel_path,rel_path_folded,role,ext,created_at,updated_at) VALUES(?1,?2,?3,'bitmap','jpg',1,1)",rusqlite::params![id,path,path.to_lowercase()]).unwrap();
        }
        for (recursive, expected) in [(false, vec![1, 2]), (true, vec![1, 2, 3])] {
            let range = Range::Directory {
                repository_id: "库".into(),
                directory: "photos/旅行".into(),
                recursive,
            };
            let mut ids = Vec::new();
            freeze(&c, "库", &range, |page| {
                assert!(page.len() <= 500);
                ids.extend(page.into_iter().map(|p| p.asset_id));
                Ok(())
            })
            .unwrap();
            assert_eq!(ids, expected);
        }
        assert!(Range::Selected { photos: vec![] }.repositories().is_err());
        assert!(
            Range::Directory {
                repository_id: "库".into(),
                directory: "photos/../a".into(),
                recursive: false
            }
            .repositories()
            .is_err()
        );
    }
}
