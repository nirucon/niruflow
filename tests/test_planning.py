from niruflow.core.planning import preview_rules

def test_disabled_rule_is_not_eligible(tmp_path):
    x=preview_rules({'rules':[{'name':'Example','path':str(tmp_path),'enabled':False,'automatic':True}]})
    assert x[0]['status']=='disabled'

def test_enabled_regular_directory(tmp_path):
    x=preview_rules({'rules':[{'name':'Example','path':str(tmp_path),'enabled':True,'automatic':True}]})
    assert x[0]['status']=='eligible for scheduler'
