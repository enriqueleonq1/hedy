from sqlparse.utils import strip_line_comments


def test_strips_comments():
    assert strip_line_comments("select 1 -- uno\nfrom t") == "select 1 \nfrom t"
    assert strip_line_comments("-- todo\n") == "\n"
    assert strip_line_comments("select 2") == "select 2"


def test_keeps_dashes_in_strings():
    assert strip_line_comments("select '--no' -- si") == "select '--no' "
