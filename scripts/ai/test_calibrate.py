import unittest
from calibrate import metrics, select
class CalibrationTests(unittest.TestCase):
    def test_empty_predictions_never_prove_precision(self):
        self.assertIsNone(select([(0, .5), (1, .1)]))
        self.assertIsNone(metrics([(1,.5)],1)['precision'])
        self.assertEqual(metrics([],1)['positives'],0)
    def test_multilabel_threshold_is_not_softmax_or_top_one(self):
        self.assertEqual(select([(1,.7),(1,.6),(0,.1)]),.6)
        self.assertEqual(metrics([(1,.7),(0,.8)],.6)['precision'],.5)
    def test_precision_first_tradeoff_is_bounded_and_not_tuned_on_validation(self):
        pairs=[(1,.9),(1,.8),(1,.7),(1,.6),(1,.5),(0,.4),(1,.3)]
        threshold=select(pairs,.85,.5,5)
        self.assertEqual(threshold,.5)
        self.assertIsNone(select([(1,.9)],.85,.5,5))
        for target,beta,count in [(0,.5,5),(.85,0,5),(.85,float('nan'),5),(.85,.5,0)]:
            with self.assertRaises(ValueError):select(pairs,target,beta,count)

if __name__=='__main__':unittest.main()
