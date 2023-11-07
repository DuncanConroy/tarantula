# Tarantula - an event-driven web-crawler written in Rust
The crawler can be given tasks and will start crawling a website until a specified depth.
The results can be sent to a callback endpoint for further processing.
It respects robots.txt, rate-limiting, and works multithreaded.
This is the first project I did with the Rust language.
The structure is different from what I would produce today, because I learned so much on the way.
The development took several months.
The learning led me to rewrite this completely from scratch for a new project.

#### The best thing about tarantulas is that they don't even spin webs to catch their food :)

Requires openssl libssl-dev or rust-tokio-native-tls+default-devel.noarch
Requires rust nightly

## Trigger
Once the server is running, e.g., on http://127.0.0.1:8000, to fire up a new crawl task, send the RunConfig
structure via PUT to the /crawl endpoint: http://127.0.0.1:8000/crawl

RunConfig structure:
{
"url": "https://example.com",
"ignore_redirects": false,
"maximum_redirects": 10,
"maximum_depth": 16,
"ignore_robots_txt": false,
"keep_html_in_memory": false,
"user_agent": "testbanane",
"callback": "https://yourhost/crawl-results"
}

The callback inside the RunConfig will be called with POST and the structure of PageResponse (page_loader::PageResponse)
After a few seconds, the results should appear on the console and at the endpoint (hopefully)
