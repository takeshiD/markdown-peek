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

    var RECONNECT_INTERVAL_MS = 3000;
    var reconnectTimer = null;

    function connectWebSocket() {
        var wsUrl = "ws://" + window.location.host + "/ws";
        console.log("Creating WebSocket: " + wsUrl);
        var socket = new WebSocket(wsUrl);

        socket.onopen = function(event) {
            console.log("WebSocket open: " + event.type);
            if (reconnectTimer !== null) {
                clearTimeout(reconnectTimer);
                reconnectTimer = null;
            }
        };

        socket.onmessage = function(event) {
            console.log("WebSocket message: " + event.data);
            if (event.data === "reload") {
                socket.close();
                window.location.reload();
            }
        };

        socket.onerror = function(event) {
            console.log("WebSocket error: " + event.type);
        };

        socket.onclose = function(event) {
            console.log("WebSocket closed (code=" + event.code + "). Reconnecting in " + RECONNECT_INTERVAL_MS + "ms...");
            reconnectTimer = setTimeout(connectWebSocket, RECONNECT_INTERVAL_MS);
        };
    }

    connectWebSocket();
}());
