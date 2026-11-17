program StdlibStringComparer;

// TStringComparer (collections.md 10.1): Ordinal is the default equality of strings;
// OrdinalIgnoreCase folds the ASCII letters only, like SameText, and gives the strings it calls
// equal the same hash. Letters outside ASCII are not folded, and no letter becomes two.

uses System.SysUtils, System.Generics.Defaults;

procedure Show(const C: IEqualityComparer<string>; const A, B: string);
begin
  WriteLn(A, ' / ', B, ': ', C.Equals(A, B), ' ', C.GetHashCode(A) = C.GetHashCode(B));
end;

var
  C: IEqualityComparer<string>;
begin
  WriteLn('Ordinal');
  C := TStringComparer.Ordinal;
  Show(C, 'abc', 'abc');
  Show(C, 'abc', 'ABC');
  WriteLn(C.GetHashCode('abc') = KeyHash('abc'));

  WriteLn('OrdinalIgnoreCase');
  C := TStringComparer.OrdinalIgnoreCase;
  Show(C, 'abc', 'abc');
  Show(C, 'Hello, World', 'hELLO, wORLD');
  Show(C, 'abc', 'abd');
  Show(C, '', '');
  Show(C, '[', '{');
  Show(C, 'é', 'É');
  Show(C, 'straße', 'STRASSE');
  Show(C, 'Ａ', 'ａ');
end.
