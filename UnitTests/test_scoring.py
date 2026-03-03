import unittest
from scoring_logic import calculate_impact_severity

class TestImpactScoring(unittest.TestCase):
    
    def test_low_impact(self):
        # Very low values across the board
        result = calculate_impact_severity(0.1, 1, 1)
        self.assertEqual(result["tier"], "Low")
        self.assertTrue(0 <= result["score"] <= 25)

    def test_critical_impact(self):
        # Very high values
        # PageRank 3.0+, Churn 50+, Dependents 100+
        result = calculate_impact_severity(4.0, 60, 120)
        self.assertEqual(result["tier"], "Critical")
        self.assertTrue(76 <= result["score"] <= 100)

    def test_medium_impact_threshold(self):
        # Values that should land in the middle
        # Dependents: 30/100 (0.12), PageRank: 1.5/3 (0.15), Churn: 10/50 (0.06) -> ~33
        result = calculate_impact_severity(1.5, 10, 30)
        self.assertEqual(result["tier"], "Medium")
        self.assertTrue(26 <= result["score"] <= 50)

    def test_high_impact_threshold(self):
        # Values that should land in the high range (51-75)
        # Score calculation: (0.7*0.4) + (0.66*0.3) + (0.6*0.3) = ~0.66 -> 66
        result = calculate_impact_severity(2.0, 30, 70)
        self.assertEqual(result["tier"], "High")
        self.assertTrue(51 <= result["score"] <= 75)

    def test_zero_values(self):
        # Handling zeros safely
        result = calculate_impact_severity(0.0, 0, 0)
        self.assertEqual(result["score"], 0)
        self.assertEqual(result["tier"], "Low")

if __name__ == '__main__':
    unittest.main()
