from cookiecutter.utils import slugify_project_name


def test_slug():
    assert slugify_project_name("Mi Proyecto 2!") == "mi-proyecto-2"
    assert slugify_project_name("  --Hola__Mundo--  ") == "hola-mundo"
    assert slugify_project_name("abc") == "abc"
    assert slugify_project_name("¿?") == ""
