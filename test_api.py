import urllib.request as r
import json
import urllib.parse
url = 'http://localhost:8080/api/analyze/impact?file_path=' + urllib.parse.quote('src/components/ResearchStrategies.js')
print(json.loads(r.urlopen(url).read()))
