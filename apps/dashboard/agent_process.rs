use std::{
    io::{self, BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

pub(super) struct AgentProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    output: Receiver<String>,
    exited: bool,
}

impl AgentProcess {
    pub(super) fn start(program: &str, args: &[&str]) -> io::Result<Self> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child
            .stdin
            .take()
            .expect("piped child stdin is available after spawning");
        let stdout = child
            .stdout
            .take()
            .expect("piped child stdout is available after spawning");
        let stderr = child
            .stderr
            .take()
            .expect("piped child stderr is available after spawning");
        let (sender, output) = mpsc::channel();

        spawn_reader(stdout, sender.clone(), "");
        spawn_reader(stderr, sender, "[stderr] ");

        Ok(Self {
            child,
            stdin: Some(stdin),
            output,
            exited: false,
        })
    }

    pub(super) fn send(&mut self, message: &str) -> io::Result<()> {
        let stdin = self.stdin.as_mut().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "the agent process has exited")
        })?;
        stdin.write_all(message.as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin.flush()
    }

    pub(super) fn poll(&mut self) -> io::Result<Vec<String>> {
        let mut output = self.output.try_iter().collect::<Vec<_>>();
        if !self.exited && self.child.try_wait()?.is_some() {
            self.exited = true;
            self.stdin = None;
            output.push("[process exited]".to_owned());
        }
        Ok(output)
    }

    pub(super) fn stop(&mut self) -> io::Result<()> {
        if self.exited {
            return Ok(());
        }

        if self.child.try_wait()?.is_none() {
            self.child.kill()?;
            self.child.wait()?;
        }
        self.stdin = None;
        self.exited = true;
        Ok(())
    }

    pub(super) fn is_running(&self) -> bool {
        !self.exited
    }
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn spawn_reader(reader: impl Read + Send + 'static, sender: Sender<String>, prefix: &'static str) {
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let output = format!("{prefix}{}", line.trim_end_matches(['\r', '\n']));
                    if sender.send(output).is_err() {
                        break;
                    }
                    line.clear();
                }
                Err(error) => {
                    let _ = sender.send(format!("{prefix}[output error] {error}"));
                    break;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use std::{
        thread,
        time::{Duration, Instant},
    };

    use super::AgentProcess;

    #[test]
    fn process_sends_input_collects_output_and_stops() {
        let mut process = AgentProcess::start("/bin/cat", &[]).unwrap();
        assert!(process.is_running());

        process.send("Merhaba DragonsTUI").unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut output = Vec::new();
        while Instant::now() < deadline && output.is_empty() {
            output.extend(process.poll().unwrap());
            if output.is_empty() {
                thread::sleep(Duration::from_millis(10));
            }
        }

        assert_eq!(output, ["Merhaba DragonsTUI"]);
        process.stop().unwrap();
        assert!(!process.is_running());
    }
}
