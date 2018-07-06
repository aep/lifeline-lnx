extern crate ssh2;
extern crate tokio;
extern crate nix;
extern crate tokio_file_unix;
extern crate libc;

use std::net::{TcpStream};
use ssh2::Session;
use std::io::{self,  Write, Read};
use std::path::Path;
use std::thread;
use nix::poll;
use std::os::unix::io::AsRawFd;




fn mainloop() {
    // Connect to the local SSH server
    let tcp = TcpStream::connect("127.0.0.1:22").unwrap();
    let mut sess = Session::new().unwrap();
    sess.handshake(&tcp).unwrap();
    sess.userauth_pubkey_file("aep", None, Path::new("/home/aep/.ssh/id_rsa"), None);

    assert!(sess.authenticated());


    let mut channel = sess.channel_session().unwrap();
    channel.request_pty("vt220", None, None).unwrap();
    channel.shell().unwrap();
    sess.set_blocking(false);

    let mut stdout = io::stdout();
    let stdin = tokio_file_unix::raw_stdin().unwrap();
    let mut stdin = tokio_file_unix::File::new_nb(stdin).unwrap();

    let stdin_fd   = stdin.as_raw_fd();
    let channel_fd = tcp.as_raw_fd();


    loop {

        let mut fds = [
            poll::PollFd::new(stdin_fd, poll::EventFlags::POLLIN),
            poll::PollFd::new(channel_fd, poll::EventFlags::POLLIN),
        ];

        poll::poll(&mut fds, -1).unwrap();

        let mut buf = [0;1024];

        if fds[0].revents() == Some(poll::EventFlags::POLLIN) {
            loop {
                let len = match stdin.read(&mut buf) {
                    Ok(len) =>  len,
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock  => break,
                    Err(e) => {
                        println!("{}", e);
                        return;
                    }
                };
                if len < 1 {
                    return;
                }
                channel.write(&mut buf[..len]);
            }
        }

        if fds[1].revents() == Some(poll::EventFlags::POLLIN) {
            loop {
                let len = match channel.read(&mut buf) {
                    Ok(len) =>  len,
                    Err(e) => {
                        if io::Error::last_os_error().kind()  == io::ErrorKind::WouldBlock {
                            break;
                        } else {
                            println!("{}", e);
                            return;
                        }
                    }
                };
                if len < 1 {
                    return;
                }
                stdout.write(&mut buf[..len]);
                stdout.flush();
            }
        }
    }


    channel.wait_close();
    println!("remote shell exit {}", channel.exit_status().unwrap());
}

fn main() {

    let stdin = tokio_file_unix::raw_stdin().unwrap();
    let stdin_fd   = stdin.as_raw_fd();

    let mut termios = nix::sys::termios::tcgetattr(stdin_fd).unwrap();
    let saved_termios = termios.clone();
    set_raw_mode(stdin_fd);


    mainloop();

    nix::sys::termios::tcsetattr(stdin_fd,
                                 nix::sys::termios::SetArg::TCSADRAIN,
                                 &saved_termios).unwrap();

}


fn reset_pty(saved_termios: ()) {
}


fn set_raw_mode(fd: i32) -> Result<(), nix::Error> {
    use nix::sys::termios::{InputFlags, LocalFlags};

    let mut termios = nix::sys::termios::tcgetattr(fd)?;

    termios.input_flags |= InputFlags::IGNPAR;    // Ignore framing errors and parity errors.
    termios.input_flags &= !(InputFlags::ISTRIP | // (dont) Strip off eighth bit.
                             InputFlags::INLCR  | // (dont) Translate NL to CR on input.
                             InputFlags::IGNCR  | // (dont) Ignore carriage return on input.
                             InputFlags::ICRNL  | // (dont) Translate carriage return to newline on input
                             InputFlags::IXON   | // (dont) Enable XON/XOFF flow control on output.
                             InputFlags::IXANY  | // (dont) Typing any character will restart stopped output.
                             InputFlags::IXOFF ); // (dont) Enable XON/XOFF flow control on input.

    termios.output_flags &= !(nix::sys::termios::OutputFlags::OPOST); // (dont) Enable output processing.
    termios.control_flags |= (nix::sys::termios::ControlFlags::CS8);  // char is 8 bit

    termios.local_flags &= !(LocalFlags::ISIG // (dont) generate control signals
                              | LocalFlags::ICANON // (dont) Enable canonical mode
                              | LocalFlags::ECHO  // (dont) Echo input characters
                              | LocalFlags::ECHOE  // (dont) enable erase char
                              | LocalFlags::ECHOK  // (dont) enable erase line
                              | LocalFlags::ECHONL // (dont) echo the NL character even if ECHO is not set.
                             );

    termios.control_chars[libc::VMIN]  = 1;
    termios.control_chars[libc::VTIME] = 0;

    nix::sys::termios::tcsetattr(fd,
                                 nix::sys::termios::SetArg::TCSAFLUSH,
                                 &termios)
}




