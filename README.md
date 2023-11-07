# Tarantula: An Event-Driven Multithreaded Web Crawler
Welcome to Tarantula, an event-driven, multithreaded web crawler built in Rust.
Tarantula is designed to efficiently crawl websites, respecting robots.txt, rate-limiting rules,
and delivering results to callback endpoints for further processing.
It's a project that has been crafted over several months and is the culmination of my journey into Rust programming.

## Key Features
- **Event-Driven Crawling:** Tarantula can be provided with tasks, and it will initiate crawling a website to a specified depth, utilizing an event-driven architecture for efficiency and responsiveness.

- **Respects Robots.txt:** Tarantula abides by robots.txt rules, ensuring that it respects website-specific crawling restrictions.

- **Multithreaded Performance:** Leveraging the power of multithreading, Tarantula optimizes its crawling process, making it faster and more parallelized.

## A Learning Journey

Tarantula is more than just a web crawler; it's a testament to my journey in Rust programming.
This project reflects my progress, lessons learned, and the evolution of my coding skills.
The structure you see here is a result of this learning process,
and I've since applied this knowledge to newer projects.

## Getting Started

To get started with Tarantula, check out the code and compile it using rust nightly.
`cargo run` should start the process and open up a server on port 8088.
Feel free to explore the code and adapt it to your specific needs.

#### The best thing about tarantulas is that they don't even spin webs to catch their food :)

Requires openssl libssl-dev or rust-tokio-native-tls+default-devel.noarch
Requires rust nightly

## Trigger
Once the server is running, e.g., on http://127.0.0.1:8088, to fire up a new crawl task, send the RunConfig
structure via PUT to the /crawl endpoint: http://127.0.0.1:8088/crawl

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

## Contributing

This project is not actively maintained or developed further.
I have decided to use tarantula and the learning experience to build a new project completely from scratch
(closed source).
I appreciate all kinds of feedback and collaboration.

Thank you for visiting Tarantula, and I hope you find it valuable for your web crawling needs.
Enjoy exploring the world of web data with this Rust-powered crawler.
Please make sure, while using tarantula, to not stress the servers you are crawling.
Be respectful and crawl responsibly.
