program LexLiterals;
var
  N: Int64;
  S: string;
begin
  N := 18446744073709551616;
  N := $1FFFFFFFFFFFFFFFF;
  N := 18446744073709551615;
  S := '''
    hello
    world x''';
  S := '''
  bad
 ''';
  S := '''
    ok
    ''';
  S := #$110000;
  S := #x;
  { outer { inner } rest }
  WriteLn(N, S); *)
end.
