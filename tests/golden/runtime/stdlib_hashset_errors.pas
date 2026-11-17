program StdlibHashSetErrors;

// The exceptions of a set name the set (collections.md 3.2, 3.3): a change during enumeration is
// EInvalidOperation, a capacity below the count ERangeError starting with `set`. A set given an
// equality comparer compares and hashes its items with it (7.4), keeping the item first added.

uses System.SysUtils, System.Generics.Defaults, System.Generics.Collections;

type
  TNames = class(THashSet<string>);

var
  S: THashSet<string>;
  N: TNames;
  X: string;

procedure Report(E: Exception);
begin
  WriteLn(E.ClassName, ': ', E.Message);
end;

begin
  S := THashSet<string>.Create(['a', 'b']);
  try
    for X in S do
      S.Add(X + X);
  except
    on E: Exception do Report(E);
  end;
  WriteLn(S.Count);
  try
    for X in S do
      S.Remove('a');
  except
    on E: Exception do Report(E);
  end;
  // Adding an item already there and removing one not there change nothing
  for X in S do
  begin
    S.Add('b');
    S.Remove('zz');
  end;
  try
    S.Capacity := 1;
  except
    on E: Exception do Report(E);
  end;
  WriteLn(S.Count, ' ', S.Capacity);
  S.Free;

  N := TNames.Create;
  N.Add('x');
  try
    for X in N do
      N.Clear;
  except
    on E: Exception do Report(E);
  end;
  N.Free;

  S := THashSet<string>.Create(TStringComparer.OrdinalIgnoreCase);
  WriteLn(S.Add('Hello'), ' ', S.Add('HELLO'), ' ', S.Contains('hello'), ' ', S.Contains('Hellö'));
  S.UnionWith(['World', 'world', 'WORLD']);
  for X in S do
    WriteLn(X);
  S.IntersectWith(['WORLD']);
  for X in S do
    WriteLn(X);
  WriteLn(S.Remove('wOrLd'), ' ', S.Count);
  S.Free;
end.
