# confirmo-utils

This crate is part of a larger set of microservices for a product focused on providing a centralized and secure means of communication for lawyers and clients to prevent scams.

This shared library ensures consistent validation logic across all services.

## Installation

To use this crate in your project, add it as a git dependency in your `Cargo.toml` file:

```toml
[dependencies]
confirmo-utils = { git = "https://github.com/your-org/confirmo-utils.git", tag = "v0.0.1" }
```

Replace the `git` URL with the actual repository URL.

## Usage

### Email Validation

To validate an email address, use the `validate_email` function:

```rust
use confirmo_utils::email::validate_email;

fn main() {
    let email = "test@example.com";
    match validate_email(email) {
        Ok(_) => println!("Email is valid"),
        Err(e) => println!("Invalid email: {:?}", e),
    }
}
```

### Password Validation

To validate a password, use the `validate_password` function:

```rust
use confirmo_utils::password::validate_password;

fn main() {
    let password = "Password1!";
    match validate_password(password) {
        Ok(_) => println!("Password is valid"),
        Err(e) => println!("Invalid password: {}", e),
    }
}
```

## Running Tests

To run the tests for this crate, use the following command:

```bash
cargo test
```
