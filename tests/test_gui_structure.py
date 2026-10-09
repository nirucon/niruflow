import ast
import unittest
from pathlib import Path

class GuiStructureTests(unittest.TestCase):
    def test_all_invoked_window_methods_exist(self):
        tree=ast.parse((Path(__file__).resolve().parents[1]/'niruflow/desktop/app.py').read_text())
        window=next(n for n in ast.walk(tree) if isinstance(n,ast.ClassDef) and n.name=='Window')
        methods={n.name for n in window.body if isinstance(n,(ast.FunctionDef,ast.AsyncFunctionDef))}
        init=next(n for n in window.body if isinstance(n,ast.FunctionDef) and n.name=='__init__')
        calls={n.func.attr for n in ast.walk(init) if isinstance(n,ast.Call) and isinstance(n.func,ast.Attribute) and isinstance(n.func.value,ast.Name) and n.func.value.id=='self'}
        self.assertEqual(calls-methods-{"setCentralWidget","resize","setWindowTitle","statusBar"},set())
