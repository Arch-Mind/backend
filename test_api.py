import urllib.request as r
import json
import urllib.parse
import sys
try:
    url = 'http://localhost:8080/api/analyze/impact?file_path=' + urllib.parse.quote('src/components/ResearchStrategies.js')
    req = r.urlopen(url)
    print(json.loads(req.read()))
except Exception as e:
    print(e)
