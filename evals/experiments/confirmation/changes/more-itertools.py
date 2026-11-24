import more_itertools


def test_sums():
    assert list(more_itertools.pairwise_sums([1, 2, 3])) == [3, 5]
    assert list(more_itertools.pairwise_sums(iter([10, -10, 5, 5]))) == [0, -5, 10]


def test_short():
    assert list(more_itertools.pairwise_sums([])) == []
    assert list(more_itertools.pairwise_sums([7])) == []
