import urllib.request as r
import json
import urllib.parse
try:
    url = 'http://localhost:8080/api/analyze/impact?file_path=' + urllib.parse.quote('src/components/ResearchStrategies.js')
    req = r.urlopen(url)
    data = json.loads(req.read())
    with open('output.json', 'w') as f:
        json.dump(data, f, indent=2)
except Exception as e:
    with open('output.json', 'w') as f:
        f.write(str(e))
