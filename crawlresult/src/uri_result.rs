// use chrono::DateTime;
//
// struct ResponseTimings {
//     request_start_time: DateTime,
//     request_complete_time: DateTime,
//     request_connection_confirmed_time: DateTime,
// }

struct Link {
    uri: String,
    belonging: LinkBelonging,
}

enum LinkBelonging {
    Internal,
    External,
}
#[derive(Debug)]
pub struct UriResult<'a> {
    // TODO: implement -- response_timings:ResponseTimings,
    pub links: Vec<&'a str> //TODO: should be Link
}