# Project

This is my submission for the "Rinha de Backend 2025" (2025 Backend Showdown) competition. And it' also my 2nd participation! The goal here is to lern Rust while bringing concepts from Software Engineering to imporve the performance of our app, given this year's competition context.

## Development

We'll start by developing the simplest application that can handles the requests/responses and processing of the data correctly. After that, the idea is to use our knowledge of computer architecture concepts like memory management or even 3rd party tools like Tokio's Tracing library, to identiify performance bottlenecks and improve them!

To get things started our objective will be to deploy one instance of the Rust web backend with a Redis instance and an Nginx load-balancer. Also, we will limit ourselves to using alway the default Payment Processor instance. Clearly not the best scenario but we want to track our improvement.

Next we will craft an algorithm to select between instances of the Payment Processor. After that we can experiment with 2  instances of the backend web server. Finally, we can use an in-memory in-process db. Since we want to achieve the best performance, regardless of concepts like availability and persistency of data in our backend, the fatest access we can get is storing things in the same process so we avoid serialization and network overhead when using Redis (we'll see how much we can save by doing this).
