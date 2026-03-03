def calculate_impact_severity(pagerank: float, churn: int, dependent_count: int) -> dict:
    """
    Calculates a risk score (0-100) and maps it to a severity tier based on:
    - PageRank (Normalized so average node = 1.0, highly central > 3.0)
    - Commit churn (number of times the file has changed)
    - Dependent count (number of upstream components affected)
    """
    # Normalize inputs (applying arbitrary caps for the sake of the algorithm)
    # Assume max churn of ~50 is very high, max dependents of ~100 is very high
    normalized_pagerank = min(pagerank / 3.0, 1.0) # PageRank is scaled by N nodes, average is 1.0
    normalized_churn = min(churn / 50.0, 1.0)
    normalized_dependents = min(dependent_count / 100.0, 1.0)
    
    # Weighted Score (0 to 1) 
    # Weights: 40% Dependents, 30% PageRank, 30% Churn
    raw_score = (normalized_dependents * 0.4) + (normalized_pagerank * 0.3) + (normalized_churn * 0.3)
    
    # Scale to 0-100
    final_score = round(raw_score * 100)
    
    # Map to Severity Tiers exactly as requested:
    # 0-25: Low, 26-50: Medium, 51-75: High, 76-100: Critical
    if final_score > 75:
        tier = "Critical"
    elif final_score > 50:
        tier = "High"
    elif final_score > 25:
        tier = "Medium"
    else:
        tier = "Low"
        
    return {
        "score": final_score,
        "tier": tier
    }
