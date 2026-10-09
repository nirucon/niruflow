from niruflow.modules.insights import project_insights, protected_location

def test_protected_unknown_and_patch():
    x=project_insights([{'kind':'patch','status':'protected'},{'kind':'unknown','status':'protected'},{'kind':'release','status':'review'}])
    assert x['total']==3 and x['protected']==2 and x['review']==1

def test_protected_location():
    assert protected_location('/home/user/Nextcloud/project')
    assert not protected_location('/home/user/Downloads/file.zip')
