import os
os.system("docker logs archmind-graph-engine --tail 50 > logs.txt 2>&1")
