function initializeMermaid() {
    window.mermaid.initialize({ startOnLoad: false, isLight: "default" });
    window.mermaid.run({ querySelector: "code.language-mermaid" });
}

function initializeMathJax() {
    window.MathJax.typeset();
}

(function() {
    initializeMermaid();
    // initializeMathJax();
    (async function() {
        const socket = new WebSocket("http://127.0.0.1:3000/ws");
        console.log("Create webscoket");
        socket.onopen = function (event) {
            console.log(`Open: ${event}`);
        };
        socket.onmessage = function (event) {
            console.log(`Received: ${event}`);
            if (event.data === "reload") {
                socket.close();
                window.location.reload();
            }
        };
        socket.onerror = function (event) {
            console.log(`Error: ${event}`);
        };
        socket.onclose = function (event) {
            console.log(`Close: ${event}`);
        };
    })();
}());
