import unittest
from thread_switch_report import parse_events, report, validate_plan
class ReportTests(unittest.TestCase):
    def test_provisional_and_ready_are_separate(self):
        text = """INFO thread switch frame committed thread_id=a cached=Some(false) first_frame=true ready=false elapsed_us=100 draw_us=20
INFO thread switch frame committed thread_id=a cached=Some(false) first_frame=false ready=true elapsed_us=300 draw_us=30
INFO unrelated elapsed_us=1 draw_us=1
INFO thread switch frame committed broken
"""
        result = report(parse_events(text))
        self.assertEqual(result["unique_threads"], 1)
        self.assertEqual(result["first_frame"], {"samples":1,"p50_us":100,"p95_us":100,"max_us":100})
        self.assertEqual(result["ready_frame"], {"samples":1,"p50_us":300,"p95_us":300,"max_us":300})
    def test_plan_requires_order_and_completion(self):
        events = [{"thread_id":"a", "ready":True}, {"thread_id":"b", "ready":True}]
        self.assertTrue(validate_plan(events, ["a","b"], "thread switch probe complete")["passed"])
        self.assertFalse(validate_plan(events, ["b","a"], "thread switch probe complete")["passed"])
        self.assertFalse(validate_plan(events, ["a","b"], "")["passed"])
        self.assertFalse(validate_plan([], ["a","b"], "thread switch probe complete")["passed"])
    def test_missing_measurement_is_not_zero_latency(self):
        self.assertEqual(report([])["first_frame"], {"samples":0})
    def test_ready_frames_are_grouped_by_cache_outcome(self):
        text = "\n".join(
            f"{prefix} thread switch frame committed thread_id=a cached={cached} first_frame=true ready={ready} elapsed_us={elapsed} draw_us=1"
            for prefix, cached, ready, elapsed in [
                ("INFO", "Some(true)", "true", 10),
                ("INFO", "Some(false)", "true", 200),
                ("INFO", "None", "true", 50),
                ("INFO", "Some(true)", "false", 999),
            ])
        result = report(parse_events(text))["ready_by_cache"]
        self.assertEqual({key:value["samples"] for key,value in result.items()},
            {"hit":1,"miss":1,"unknown":1})
        self.assertEqual(result["hit"]["max_us"], 10)
        self.assertEqual(result["miss"]["max_us"], 200)
if __name__ == "__main__": unittest.main()
