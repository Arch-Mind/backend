const http = require('http');

http.get('http://127.0.0.1:8080/api/analyze/impact?file_path=src/components/ResearchStrategies.js', (res) => {
    let data = '';
    res.on('data', (chunk) => {
        data += chunk;
    });
    res.on('end', () => {
        console.log(JSON.parse(data));
    });
}).on("error", (err) => {
    console.log("Error: " + err.message);
});
