import networkx as nx

G = nx.DiGraph()
G.add_edge("A", "B")
G.add_edge("C", "B")
G.add_edge("D", "B")
G.add_edge("E", "B")
G.add_edge("F", "B")
pr = nx.pagerank(G)
print("PageRanks:", pr)
print("Scaled PageRanks:", {k: v * len(G.nodes()) for k, v in pr.items()})
