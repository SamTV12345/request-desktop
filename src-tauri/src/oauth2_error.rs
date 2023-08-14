pub struct OAuth2Error{
    message: String,
}

impl OAuth2Error{
    pub fn new(message: String) -> OAuth2Error{
        OAuth2Error{
            message
        }
    }
}
