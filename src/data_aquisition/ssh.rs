use std::net::TcpStream;
use std::io::Read;
use std::sync::Arc;
use ssh2::Session;
use tokio::sync::Mutex;

use thiserror::Error;

pub struct SshClient {
    username: String,
    host: String,
    password: Option<String>,
    port: u16,
    session: Option<Arc<Mutex<ssh2::Session>>>
}

#[derive(Debug, Error)]
pub enum SshError {
    #[error("TCP error: {0}")]
    TcpError(String),
    #[error("SSH error: {0}")]
    SshError(String),
    #[error("SSH authentication error: {0}")]
    SshAuthError(String),
    #[error("Command execution error: {0}")]
    CommandError(String),
    #[error("Async error: {0}")]
    AsyncError(String),
}

impl SshClient {
    pub fn new_with_password(username: String, host: String, password: String, port: u16) -> Self {
        Self {
            username,
            host,
            password: Some(password),
            port,
            session: None
        }
    }
    
    // Move your sync logic here:
    fn connect_sync_inner(username: String, host: String, password: Option<String>, port: u16) -> Result<Session, SshError> {
        let tcp = TcpStream::connect(format!("{}:{}", host, port)).map_err(|e| SshError::TcpError(e.to_string()))?;
        let mut session = ssh2::Session::new().map_err(|e| SshError::SshError(e.to_string()))?;
        session.set_tcp_stream(tcp);
        session.handshake().map_err(|e| SshError::SshError(e.to_string()))?;
        if let Some(password) = password {
            session.userauth_password(&username, &password).map_err(|e| SshError::SshAuthError(e.to_string()))?;
        }
        if !session.authenticated() {
            return Err(SshError::SshAuthError("Authentication failed".to_string()));
        }
        Ok(session)
    }
    
    pub async fn connect(&mut self) -> Result<(), SshError> {
        if self.session.is_some() {
            return Err(SshError::SshError("Already connected".to_string()));
        }
        let username = self.username.clone();
        let host = self.host.clone();
        let password = self.password.clone();
        let port = self.port;
        let session = tokio::task::spawn_blocking(move || {
            SshClient::connect_sync_inner(username, host, password, port)
        })
        .await
        .map_err(|e| SshError::AsyncError(e.to_string()))?
        ?;
        self.session = Some(Arc::new(Mutex::new(session)));
        Ok(())
    }
    
    fn execute_command_sync(session: &mut ssh2::Session, command: &str) -> Result<String, SshError> {
        let mut channel = session.channel_session().map_err(|e| SshError::SshError(e.to_string()))?;
        channel.exec(command).map_err(|e| SshError::CommandError(e.to_string()))?;
        let mut output = String::new();
        channel.read_to_string(&mut output).map_err(|e| SshError::CommandError(e.to_string()))?;
        channel.wait_close().map_err(|e| SshError::SshError(e.to_string()))?;
        Ok(output)
    }
    
    pub async fn execute_command(&self, command: &str) -> Result<String, SshError> {
        let command = command.to_string();
        let session_mutex = match &self.session {
            Some(s) => s.clone(),
            None => return Err(SshError::SshError("Session not initialized".to_string())),
        };
        let result: Result<String, SshError> = tokio::task::spawn_blocking(move || {
            let mut session = session_mutex.blocking_lock();
            Self::execute_command_sync(&mut session, &command)
        })
        .await
        .map_err(|e| SshError::AsyncError(e.to_string()))?;
        result
    }
    
    pub fn is_connected(&self) -> bool {
        self.session.is_some()
    }
    
    pub async fn close(self) -> Result<(), SshError> {
        if let Some(session) = self.session {
            let session = session.lock().await;
            session.disconnect(Some(ssh2::DisconnectCode::ByApplication), "", None).map_err(|e| SshError::SshError(e.to_string()))?;
        }
        Ok(())
    }
}

mod tests {
    use super::*;
    
    fn new_r1_client() -> SshClient {
        SshClient::new_with_password(
            "client".to_string(),
            "localhost".to_string(),
            "password".to_string(),
            2221)
    }
    
    #[tokio::test]
    async fn test_connect() {
        let mut client = new_r1_client();
        let res = client.connect().await;
        if let Err(e) = &res {
            println!("Error connecting: {}", e);
        }
        assert!(res.is_ok());
    }
    
    #[tokio::test]
    async fn test_execute_command() {
        let mut client = new_r1_client();
        let res = client.connect().await;
        if let Err(e) = &res {
            println!("Error connecting: {}", e);
        }
        assert!(res.is_ok());
        
        let output = client.execute_command("echo \"Hello!\"").await;
        dbg!(&output);
        assert!(output.is_ok());
        let output = output.unwrap();
        assert!(!output.is_empty());
        assert_eq!(output, "Hello!\n");
    }
}
