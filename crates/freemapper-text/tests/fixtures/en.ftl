hello = Hello
greet = Hello, { $name }
files = { $count ->
    [one] one file
   *[other] { $count } files
}
-brand = Fixture
about = About { -brand }
