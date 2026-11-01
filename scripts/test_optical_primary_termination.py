"""Original MIT. Closed-source proof inventory; no GPU or new admission policy."""
import hashlib
import json
from pathlib import Path
import re
import struct
import unittest

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = json.loads((ROOT / "scripts/optical_primary_termination.json").read_text())
SOURCE = (ROOT / MANIFEST["reference_shader"]).read_text()


def uncomment(text):
    """WGSL comments, including nested block comments, are not source tokens."""
    result = []
    i = 0
    while i < len(text):
        if text.startswith("//", i):
            end = text.find("\n", i + 2)
            i = len(text) if end < 0 else end
        elif text.startswith("/*", i):
            depth = 1
            i += 2
            while depth:
                if i >= len(text):
                    raise AssertionError("unterminated comment")
                if text.startswith("/*", i):
                    depth += 1
                    i += 2
                elif text.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            result.append(" ")
        else:
            result.append(text[i])
            i += 1
    return "".join(result)


def functions(text):
    text = uncomment(text)
    result = {}
    for match in re.finditer(r"\bfn\s+(\w+)\s*\(", text):
        name = match.group(1)
        start = text.index("{", match.end())
        end = start + 1
        depth = 1
        while depth:
            if text[end] == "{":
                depth += 1
            elif text[end] == "}":
                depth -= 1
            end += 1
        assert name not in result
        result[name] = text[start:end]
    return result


def loop_headers(body):
    pattern = r"\bfor\s*\([^{}]*?\)|\bloop(?=\s*\{)|\bwhile\s*\([^{}]*?\)"
    return [re.sub(r"\s+", "", m.group(0)) for m in re.finditer(pattern, body)]


class TerminationProofTests(unittest.TestCase):
    def test_exact_retained_hashes_and_fixed_packet(self):
        for role in ["shader", "uniform"]:
            data = (ROOT / MANIFEST[f"reference_{role}"]).read_bytes()
            self.assertEqual(len(data), MANIFEST[f"reference_{role}_bytes"])
            self.assertEqual(hashlib.sha256(data).hexdigest(), MANIFEST[f"reference_{role}_sha256"])
        uniform = (ROOT / MANIFEST["reference_uniform"]).read_bytes()
        self.assertEqual(struct.unpack_from("<f", uniform, 9 * 16)[0], 45.0)
        self.assertEqual(struct.unpack_from("<4I", uniform, 11 * 16), (1920, 1080, 0x12345678, 0x9abcdef0))

    def test_all_functions_and_loops_are_in_the_proof(self):
        bodies = functions(SOURCE)
        self.assertEqual(list(bodies), MANIFEST["all_functions"])
        expected = {p["function"]: [re.sub(r"\s+", "", h) for h in p["headers"]] for p in MANIFEST["loops"]}
        total = {"for": 0, "loop": 0, "while": 0}
        for name, body in bodies.items():
            headers = loop_headers(body)
            self.assertEqual(headers, expected.get(name, []), name)
            for header in headers:
                total[next(k for k in total if header.startswith(k))] += 1
        self.assertEqual(total, MANIFEST["loop_totals"])

    def test_whole_declared_call_graph_is_acyclic(self):
        bodies = functions(SOURCE)
        names = set(bodies)
        calls = {name: set(re.findall(r"\b(\w+)\s*\(", body)) & names for name, body in bodies.items()}
        complete = set()

        def visit(name, active):
            self.assertNotIn(name, active, "recursive function graph")
            if name in complete:
                return
            for called in calls[name]:
                visit(called, active | {name})
            complete.add(name)

        for name in names:
            visit(name, set())
        self.assertEqual(complete, names)

    def test_profile_search_and_inclusive_loop_do_not_wrap(self):
        for count in range(2, 129):
            states = {(0, count - 1)}
            for _ in range(8):
                after = set()
                for lo, hi in states:
                    if hi <= lo + 1:
                        after.add((lo, hi))
                    else:
                        mid = (lo + hi) // 2
                        after.update([(mid, hi), (lo, mid)])
                states = after
            for lo, hi in states:
                self.assertTrue(0 <= lo <= hi <= count - 1)
                first = max(0, lo - 1)
                last = min(lo + 1, count - 2)
                self.assertLessEqual(last, 126)
                self.assertLessEqual(max(0, last - first + 1), 3)
                self.assertLess(last + 1, 2 ** 32)

    def test_source_guards_cover_every_pointer_dependent_continue(self):
        bodies = functions(SOURCE)
        for name, cap, loops in [("terrain_events", 8192, 2), ("terrain_coverage", 8192, 2), ("marker_events", 2048, 1)]:
            body = re.sub(r"\s+", "", bodies[name])
            self.assertEqual(body.count("varvisited=0u;"), 1, name)
            self.assertEqual(body.count("visited+=1u;"), loops, name)
            self.assertNotIn("visited-=", body)
            self.assertNotIn("visited=", body.replace("varvisited=0u;", ""))
            starts = re.findall(r"loop\{if(?:index>=limit|node>=stop)\{break;\}visited\+=1u;ifvisited>" + str(cap) + r"u\{(?:result.ambiguous=1u;returnresult;|return2u;)\}", body)
            self.assertEqual(len(starts), loops, name)
            self.assertLess(cap + 1, 2 ** 32)

    def test_copied_production_functions_remain_exact(self):
        primary = (ROOT / "src/desktop/shaders/refraction.wesl").read_text()
        core = primary[primary.index("fn qmul"):primary.index("fn relative_angles")]
        scene = (ROOT / "src/desktop/shaders/refraction_scene.wesl").read_text()
        scene = scene[:scene.index("fn sign_row(")].replace("constants::MATERIAL_BIND_GROUP", "0").replace("@binding(9)", "@binding(1)").replace("@binding(10)", "@binding(2)")
        self.assertIn(core, SOURCE)
        self.assertIn(scene, SOURCE)
        self.assertEqual(SOURCE.count("@compute"), 1)


if __name__ == "__main__":
    unittest.main()
