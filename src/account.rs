use std::path::{Path, PathBuf};
use std::{fs, process};

use crate::opts::AccountCommand;
use crate::project::TrampolineProject;
use anyhow::Result;
use rpassword::prompt_password;
use trampoline_sdk::account::{Account, AccountError};

pub struct AccountSubCommand {
    project: TrampolineProject,
}

impl AccountSubCommand {
    pub fn new(project: TrampolineProject) -> Self {
        AccountSubCommand { project }
    }

    pub fn process(&self, command: AccountCommand) {
        match self.process_inner(command) {
            Ok(s) => println!("{}", s),
            Err(e) => {
                eprintln!("{}", e);
                process::exit(1);
            }
        }
    }

    fn process_inner(&self, command: AccountCommand) -> Result<String, String> {
        let mgr = AccountManager::new(self.project.root_dir.join(".trampoline").join("accounts"));
        match command {
            AccountCommand::New {} => {
                let password = read_password(true, None)?;
                let account = mgr
                    .create_account(password.as_bytes())
                    .map_err(|e| format!("{}", e))?;
                Ok(format!("Account Created\n{}", json_account(&account)))
            }
            AccountCommand::Import { sk } => {
                let password = read_password(true, None)?;
                let account = mgr
                    .import_account(sk, password.as_bytes())
                    .map_err(|e| format!("{}", e))?;
                Ok(format!("Account Imported\n{}", json_account(&account),))
            }
        }
    }
}

fn read_password(repeat: bool, prompt: Option<&str>) -> Result<String, String> {
    let prompt = prompt.unwrap_or("Password: ");
    let pass = prompt_password(prompt).map_err(|err| err.to_string())?;
    if repeat {
        let repeat_pass = prompt_password("Repeat password: ").map_err(|err| err.to_string())?;
        if pass != repeat_pass {
            return Err("Passwords do not match".to_owned());
        }
    }
    Ok(pass)
}

fn json_account(account: &Account) -> String {
    let value = serde_json::json!({
        "lock_arg": account.lock_arg_hex()
    });
    serde_json::to_string_pretty(&value).unwrap()
}

pub struct AccountManager {
    pub root_dir: PathBuf,
}

impl AccountManager {
    pub fn new<P: AsRef<Path>>(root_dir: P) -> Self {
        AccountManager {
            root_dir: root_dir.as_ref().into(),
        }
    }

    /// Create an account, save the hex-formatted secret key to a file.
    /// The file name is the hex-formatted lock_arg.
    pub fn create_account(&self, password: &[u8]) -> Result<Account, AccountError> {
        let account = Account::new(password)?;
        self.write_account(&account)?;
        Ok(account)
    }

    /// Import an account using hex-formatted secret key, save it to file.
    /// The file name is the hex-formatted lock_arg.
    pub fn import_account(
        &self,
        sk_string: String,
        password: &[u8],
    ) -> Result<Account, AccountError> {
        let account = Account::from_secret(sk_string, password)?;
        self.write_account(&account)?;
        Ok(account)
    }

    fn write_account(&self, account: &Account) -> Result<(), AccountError> {
        let file = self.root_dir.join(account.lock_arg_hex());
        fs::write(file, serde_json::to_string_pretty(account)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_new_account() {
        let root = tempdir().unwrap();
        let mgr = AccountManager::new(&root);
        let account = mgr.create_account(&[]).unwrap();

        let account2 = Account::from_file(root.path().join(account.lock_arg_hex()), &[]).unwrap();
        assert_eq!(account2, account);
    }

    #[test]
    fn test_import_account() {
        let root = tempdir().unwrap();
        let mgr = AccountManager::new(&root);

        let sk_hex = "009c0df368efef6084ba35ded33f05ef2a5f4b25d7841bce77e2449be9311dba";
        let account = mgr.import_account(sk_hex.into(), &[]).unwrap();
        let keyfile = root.path().join(account.lock_arg_hex());
        assert!(keyfile.exists());
        assert_eq!(account, Account::from_file(keyfile, &[]).unwrap());
    }

    #[test]
    fn test_import_invalid_account() {
        let root = tempdir().unwrap();
        let mgr = AccountManager::new(&root);

        let sk_hex = "009c0df368efef6084ba35ded00000ef2a5f4b25d7841bce77e2449be9311dbx";
        let result = mgr.import_account(sk_hex.into(), &[]);
        assert!(result.is_err());
    }
}
