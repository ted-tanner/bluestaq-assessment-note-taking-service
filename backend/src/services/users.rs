use crate::{
    db::{self, DbPool, users as dao},
    error::Error,
    models::user::UserInfo,
};

#[derive(Clone)]
pub struct UserService {
    pub pool: DbPool,
}

pub fn validate_name(name: String) -> Result<String, Error> {
    let name = name.trim().to_owned();
    if !(1..=100).contains(&name.chars().count()) {
        return Err(Error::BadRequest("Name must contain 1–100 characters"));
    }
    Ok(name)
}

impl UserService {
    pub async fn get(&self, id: String) -> Result<UserInfo, Error> {
        db::run(self.pool.clone(), move |c| Ok(dao::get(c, &id)?)).await
    }

    pub async fn rename(&self, id: String, name: String) -> Result<UserInfo, Error> {
        let name = validate_name(name)?;
        db::run(self.pool.clone(), move |c| {
            c.immediate_transaction::<_, Error, _>(|c| {
                dao::rename(c, &id, &name)?;
                Ok(dao::get(c, &id)?)
            })
        })
        .await
    }

    pub async fn search(&self, owner: String, query: String) -> Result<Vec<UserInfo>, Error> {
        let query = query.trim();
        if !(2..=100).contains(&query.chars().count()) {
            return Err(Error::BadRequest("Search must contain 2–100 characters"));
        }
        let query = query.to_owned();
        db::run(self.pool.clone(), move |c| {
            Ok(dao::search(c, &owner, &query)?)
        })
        .await
    }
}
