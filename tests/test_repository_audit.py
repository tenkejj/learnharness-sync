from pathlib import Path
import os
import subprocess
import sys
import tempfile
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parent.parent/'scripts'))
from audit_repository import audit

class RepositoryAuditTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.root=Path(self.tmp.name).resolve()
        self.env=dict(os.environ,GIT_CONFIG_GLOBAL=os.devnull,GIT_CONFIG_NOSYSTEM='1',
                      GIT_AUTHOR_NAME='HarnessSync Test',GIT_AUTHOR_EMAIL='test@example.invalid',
                      GIT_COMMITTER_NAME='HarnessSync Test',GIT_COMMITTER_EMAIL='test@example.invalid')
        self.git('init','-q')
    def tearDown(self):self.tmp.cleanup()
    def git(self,*args):subprocess.run(['git','-C',str(self.root),*args],env=self.env,check=True,capture_output=True)
    def test_staged_secret_cannot_be_hidden_by_clean_worktree(self):
        path=self.root/'example.txt';path.write_text('ghp_'+'Z'*40)
        self.git('add','example.txt');path.write_text('sanitized worktree')
        result=audit(self.root)
        self.assertTrue(any(f['scope']=='index' for f in result['findings']))
    def test_deleted_historical_private_file_is_detected(self):
        (self.root/'config.xml').write_text('<configuration><apikey>invented</apikey></configuration>')
        self.git('add','.');self.git('commit','-qm','invented config')
        self.git('rm','config.xml');self.git('commit','-qm','remove config')
        self.assertTrue(any(f['scope']=='history' and f['file']=='config.xml' for f in audit(self.root)['findings']))
    def test_same_blob_renamed_to_private_path_still_detected(self):
        (self.root/'ordinary.txt').write_text('invented identical content')
        self.git('add','.');self.git('commit','-qm','ordinary')
        self.git('mv','ordinary.txt','config.xml');self.git('commit','-qm','private path')
        self.assertTrue(any(f['scope']=='history' and f['file']=='config.xml' for f in audit(self.root)['findings']))
    def test_clean_unborn_repo_can_be_inspected(self):
        (self.root/'README.md').write_text('invented public fixture')
        self.assertEqual(audit(self.root)['targeted_scan'],'passed')
