"""Evaluation infrastructure checks; these do not grade an LLM's semantic accuracy."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('qualitative_eval', ROOT / 'scripts/qualitative_eval.py')
qe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qe)
sys.path.insert(0, str(ROOT / 'scripts'))
import dialogue_eval as de


class SuiteTests(unittest.TestCase):
    def test_cases_are_diverse_without_a_fixed_selected_set_oracle(self):
        suite = qe.load_suite(qe.DEFAULT_SUITE)
        self.assertEqual(len(suite['cases']), 36)
        families = {c['family'] for c in suite['cases']}
        for name in ['single_action_multiple_methods', 'single_action_document_methods',
                     'short_request_known_state', 'short_request_no_state',
                     'multiple_unprioritized_goals', 'declared_policy_conflict',
                     'untrusted_source_instructions', 'unfamiliar_custom_method']:
            self.assertIn(name, families)
        self.assertFalse(any(k.startswith('expected_') for c in suite['cases'] for k in c))

    def test_invalid_fixture_paths_and_fixed_oracles_are_refused(self):
        suite = qe.read_json(qe.DEFAULT_SUITE)
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / 'suite.json'
            suite['cases'][0]['fixture'] = '../../'
            qe.write_json(p, suite)
            with self.assertRaises(ValueError): qe.load_suite(p)
            suite = qe.read_json(qe.DEFAULT_SUITE)
            suite['cases'][0]['expected_selected'] = ['P-X']
            qe.write_json(p, suite)
            with self.assertRaises(ValueError): qe.load_suite(p)


class ArchiveTests(unittest.TestCase):
    """Synthetic records here test export integrity, never a model's behavior."""
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.run = Path(self.tmp.name) / 'run'
        private = self.run / 'controller/S01'
        qe.write_json(private / 'initial.json', {'request': 'Infrastructure fixture'})
        qe.write_json(private / 'result/observation.json', {'amh_calls': [], 'semantic_verdict': None})
        qe.write_json(self.run / 'controller.json', {'suite_sha256': 'fixture', 'amh_sha256': 'fixture',
                      'created_ns': 1, 'cases': {'S01': {'private': str(private)}}})
        routing = {}
        for label, opinion in [('A', 'One defensible approach'), ('B', 'Another defensible approach')]:
            folder = self.run / 'reviewers' / label
            qe.write_json(folder / 'initial.json', {'request': 'Infrastructure fixture'})
            (folder / 'provisional.md').write_text(opinion)
            qe.write_json(folder / 'routing.json', {'provisional_sha256': qe.sha((folder / 'provisional.md').read_bytes()),
                          'initial_sha256': qe.sha((folder / 'initial.json').read_bytes())})
            (folder / 'stage-one.txt').write_text('Synthetic stage one')
            (folder / 'stage-two.txt').write_text('Synthetic stage two')
            qe.write_json(folder / 'review.json', {'finding': opinion, 'uncertainty': 'Not an executed agent case'})
            routing['S01' + label] = {'directory': str(folder)}
        qe.write_json(self.run / 'review-routing.json', routing)
        self.out = Path(self.tmp.name) / 'archive'

    def test_export_preserves_different_opinions_without_inventing_session_facts(self):
        metadata = qe.export_evidence(self.run, self.out)
        self.assertIsNone(metadata['automatic_semantic_score'])
        self.assertEqual(metadata['session_provenance'], {})
        a = qe.read_json(self.out / 'S01/review-A.json')['review']
        b = qe.read_json(self.out / 'S01/review-B.json')['review']
        self.assertNotEqual(a['finding'], b['finding'])
        manifest = qe.read_json(self.out / 'manifest.json')
        self.assertEqual(manifest['S01/review-A.json'], qe.sha((self.out / 'S01/review-A.json').read_bytes()))
        with self.assertRaises(FileExistsError): qe.export_evidence(self.run, self.out)

    def test_incomplete_reviews_are_rejected_before_output_is_created(self):
        (self.run / 'reviewers/B/review.json').unlink()
        with self.assertRaises(FileNotFoundError): qe.export_evidence(self.run, self.out)
        self.assertFalse(self.out.exists())

    def test_changed_provisional_or_initial_evidence_is_rejected(self):
        folder = self.run / 'reviewers/A'
        original = (folder / 'provisional.md').read_bytes()
        (folder / 'provisional.md').write_text('Changed after reveal')
        with self.assertRaises(ValueError): qe.export_evidence(self.run, self.out)
        (folder / 'provisional.md').write_bytes(original)
        qe.write_json(folder / 'initial.json', {'request': 'Changed after reveal'})
        with self.assertRaises(ValueError): qe.export_evidence(self.run, self.out)
        self.assertFalse(self.out.exists())


@unittest.skipUnless(os.environ.get('AMH_EXECUTABLE') and os.name == 'posix',
                     'set AMH_EXECUTABLE to a built native CLI on a POSIX host')
class RunTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.run_dir = Path(self.tmp.name) / 'run'
        self.amh = Path(os.environ['AMH_EXECUTABLE']).resolve()
        self.controller = qe.prepare(self.amh, qe.DEFAULT_SUITE, self.run_dir, ['S01', 'S02', 'S03', 'S04'])

    def item(self, id='S01'):
        return self.controller['cases'][id]

    def test_actor_export_contains_no_controller_hints_or_prior_answers(self):
        item = self.item()
        assignment = qe.read_json(Path(item['workspace']).parent / 'assignment.json')
        self.assertEqual(set(assignment), {'request', 'workspace', 'amh'})
        prompt = Path(item['actor_prompt']).read_text()
        self.assertNotIn(item['case']['review_note'], prompt)
        self.assertNotIn(item['case']['family'], prompt)
        self.assertIn(item['case']['request'], prompt)
        self.assertNotIn('expected_selected', prompt)

    def test_real_tool_audit_records_outputs_without_semantic_scoring(self):
        item = self.item()
        r = subprocess.run([item['tool'], '--version'], cwd=item['workspace'], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0)
        audit = Path(item['private']) / 'audit'
        calls = [json.loads(x) for x in (audit / 'calls.jsonl').read_text().splitlines()]
        self.assertEqual(calls[0]['argv'], ['--version'])
        self.assertIn('amh', (audit / (calls[0]['id'] + '.stdout')).read_text())
        self.assertNotIn('semantic_verdict', calls[0])

    def test_outcome_checks_are_facts_not_an_automatic_acceptance_verdict(self):
        observation = qe.collect(self.run_dir, 'S01')
        self.assertEqual(observation['independent_outcome_checks'][0]['exit_code'], 1)
        self.assertIsNone(observation['semantic_verdict'])
        self.assertEqual(observation['actor_note_authority'], 'self_report_not_execution_proof')
        manifest = qe.read_json(Path(self.item()['private']) / 'result/evidence-manifest.json')
        self.assertIn('observation.json', manifest)

    def test_review_results_remain_hidden_until_provisional_reasoning_is_frozen(self):
        qe.collect(self.run_dir, 'S02')
        review = qe.reviewer(self.run_dir, 'S02', 'A')
        folder = Path(review['directory'])
        self.assertEqual(qe.read_json(self.run_dir / 'review-routing.json')['S02A']['directory'], str(folder))
        with self.assertRaises(FileExistsError): qe.reviewer(self.run_dir, 'S02', 'A')
        self.assertFalse((folder / 'actor').exists())
        initial = qe.read_json(folder / 'initial.json')
        self.assertNotIn('family', initial)
        self.assertNotIn('review_note', initial)
        self.assertNotIn('actor_note', initial)
        with self.assertRaises(ValueError): qe.reveal(folder)
        (folder / 'provisional.md').write_text('Revisable assessment based on initial evidence.')
        qe.reveal(folder)
        self.assertTrue((folder / 'actor/observation.json').exists())
        self.assertEqual(qe.read_json(folder / 'routing.json')['provisional_sha256'], qe.sha((folder / 'provisional.md').read_bytes()))
        with self.assertRaises(FileExistsError): qe.reveal(folder)

    def test_known_task_is_seeded_in_product_state_not_the_actor_prompt(self):
        item = self.item('S03')
        initial = qe.read_json(Path(item['private']) / 'initial.json')
        goal = initial['current_deployment']['task']['goal']
        self.assertNotIn(goal, Path(item['actor_prompt']).read_text())
        self.assertIn(goal, (Path(item['workspace']) / '.amh/generated/AGENTS.md').read_text())
        absent = qe.read_json(Path(self.item('S04')['private']) / 'initial.json')
        self.assertIsNone(absent['current_deployment']['task'])

    def test_collect_omits_symlinks_instead_of_following_private_data(self):
        item = self.item()
        (Path(item['workspace']) / 'outside').symlink_to(self.run_dir / 'controller.json')
        observation = qe.collect(self.run_dir, 'S01')
        self.assertIn('outside', observation['omitted_symlinks'])
        self.assertFalse((Path(item['private']) / 'result/workspace/outside').exists())

    def test_every_scenario_can_be_initialized_without_model_answers(self):
        full = qe.prepare(self.amh, qe.DEFAULT_SUITE, Path(self.tmp.name) / 'all')
        self.assertEqual(len(full['cases']), 36)
        self.assertTrue(all(Path(v['workspace']).is_dir() for v in full['cases'].values()))

    def test_session_stdin_is_captured_as_observed_protocol_input(self):
        item = self.item('S04')
        def call(args, data=None):
            r = subprocess.run([item['tool'], *args], cwd=item['workspace'], input=data, capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stderr)
            return json.loads(r.stdout)
        call(['session', 'open', '--id', 'one', '--apply'])
        p = call(['session', 'prepare', '--id', 'one', '--text', 'Explain the current code.'])
        quote = [{'source': 'request', 'quote': p['request']}]
        response = {'format': 'amh.session-analysis.v1', 'request_digest': p['request_digest'], 'mode': 'review', 'reason': 'Infrastructure fixture', 'evidence': quote,
                    'assessments': [{'id': c['id'], 'item_revision': c['item_revision'], 'applicability': 'no_match', 'reason': 'Infrastructure fixture', 'evidence': quote} for c in p['candidates']]}
        handoff = json.dumps({'packet': p, 'response': response})
        call(['session', 'finish', '--input', '-', '--apply'], handoff)
        audit = Path(item['private']) / 'audit'
        calls = [json.loads(x) for x in (audit / 'calls.jsonl').read_text().splitlines()]
        self.assertEqual(json.loads((audit / calls[-1]['stdin_file']).read_text()), json.loads(handoff))

    def test_work_trace_captures_command_output_and_actual_file_changes(self):
        item = self.item()
        tool = de.make_trace(Path(item['tool']).parent, Path(item['workspace']), Path(item['private']))
        r = subprocess.run([str(tool), 'exec', sys.executable, '-c', "from pathlib import Path;Path('observed.txt').write_text('observed change');print('actual output')"], input=b'', capture_output=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        audit = Path(item['private']) / 'work-audit'
        event = json.loads((audit / 'events.jsonl').read_text())
        self.assertNotIn('observed.txt', event['before'])
        self.assertEqual(event['after']['observed.txt']['text'], 'observed change')
        self.assertIn('actual output', (audit / (event['id'] + '.stdout')).read_text())
        self.assertNotIn('semantic_verdict', event)

    def test_dialogue_freezing_requires_every_turn_before_final_review(self):
        run = Path(self.tmp.name) / 'dialogue'
        c = de.prepare(self.amh, ROOT / 'evals/dialogues.json', run)
        de.collect_turn(run, 'D03', 1)
        with self.assertRaises(ValueError): de.finalize(run, 'D03')
        de.collect_turn(run, 'D03', 2)
        de.collect_turn(run, 'D03', 3)
        result = de.finalize(run, 'D03')
        self.assertTrue((result / 'turns/1/observation.json').exists())
        manifest = qe.read_json(result / 'evidence-manifest.json')
        self.assertEqual(manifest['observation.json'], qe.sha((result / 'observation.json').read_bytes()))
        self.assertIn('turns/1/observation.json', manifest)
        self.assertEqual(len(qe.read_json(Path(c['cases']['D03']['private']) / 'initial.json')['user_messages']), 3)

    def test_resume_is_an_explicit_same_project_link_without_prior_conversation(self):
        run = Path(self.tmp.name) / 'resume'
        c = de.prepare(self.amh, ROOT / 'evals/dialogues.json', run)
        source = c['cases']['D02']
        qe.invoke(self.amh, Path(source['workspace']), 'session', 'open', '--id', 'one', '--apply')
        current = qe.invoke(self.amh, Path(source['workspace']), 'policy', 'list')
        writing = next(x for x in current['policies'] if x['item']['id'] == 'P-WRITING')
        change = run / 'pause.json'
        qe.write_json(change, {'operation': 'PAUSE', 'id': 'P-WRITING', 'item_revision': writing['item']['revision']})
        qe.invoke(self.amh, Path(source['workspace']), 'policy', 'change', '--file', str(change), '--expected-revision', str(current['revision']), '--reason', 'Fixture policy changed before resume', '--apply')
        resume = de.prepare_resume(run, self.amh, 'D02', 'D07', 'one', 'Explain the report.')
        self.assertEqual(resume['workspace'], source['workspace'])
        self.assertEqual(resume['case']['turns'], ['Explain the report.'])
        prompt = Path(resume['actor_prompt']).read_text()
        self.assertNotIn(source['case']['turns'][0], prompt)
        self.assertNotIn(source['case']['review_note'], prompt)
        initial = qe.read_json(Path(resume['private']) / 'initial.json')
        self.assertTrue(initial['resume_link']['prior_conversation_unavailable'])
        writing = next(x for x in initial['policies']['policies'] if x['item']['id'] == 'P-WRITING')
        self.assertEqual(writing['item']['lifecycle'], 'paused')
        with self.assertRaises(FileExistsError):
            de.prepare_resume(run, self.amh, 'D02', 'D07', 'one', 'Explain again.')

    def test_prepare_never_overwrites_an_existing_run(self):
        with self.assertRaises(FileExistsError):
            qe.prepare(self.amh, qe.DEFAULT_SUITE, self.run_dir, ['S01'])


if __name__ == '__main__': unittest.main()
