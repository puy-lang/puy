// TList<T>.Add is specialized for each item type (collections.md 12.1, design/generics.md 3.4):
// the bytecode of TList<Integer>.Add stores the Integer in the array as it is, and that of
// TList<string>.Add counts the string, with no boxing of the item in either.
program DumpStdlibListAdd;

uses System.Generics.Collections;

var
  L: TList<Integer>;
  S: TList<string>;
begin
  L := TList<Integer>.Create;
  L.Add(1);
  S := TList<string>.Create;
  S.Add('a');
  WriteLn(L[0], S[0]);
  S.Free;
  L.Free;
end.
