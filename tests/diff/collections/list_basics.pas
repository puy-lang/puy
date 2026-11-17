program ListBasics;

// The basic operations of TList<T> (collections.md 5.1) on Integer and string items: adding,
// inserting, deleting, reading and writing by index, and finding by IndexOf / Contains. Nothing
// here depends on the exceptions or the growth of the capacity, which differ from FPC.

uses System.SysUtils, System.Generics.Collections;

procedure ShowInts(const L: TList<Integer>);
var
  I: Integer;
begin
  Write('[');
  for I := 0 to L.Count - 1 do
  begin
    if I > 0 then
      Write(' ');
    Write(L[I]);
  end;
  WriteLn('] count=', L.Count);
end;

procedure ShowStrs(const L: TList<string>);
var
  S: string;
begin
  Write('[');
  for S in L do
    Write('<', S, '>');
  WriteLn('] count=', L.Count);
end;

var
  L: TList<Integer>;
  S: TList<string>;
begin
  L := TList<Integer>.Create;
  ShowInts(L);
  WriteLn(L.Add(10), ' ', L.Add(20), ' ', L.Add(30));
  ShowInts(L);
  L.Insert(0, 5);
  L.Insert(2, 15);
  L.Insert(L.Count, 35);
  ShowInts(L);
  L[1] := 11;
  WriteLn(L[0], ' ', L[1], ' ', L[L.Count - 1]);
  L.Delete(0);
  L.Delete(L.Count - 1);
  ShowInts(L);
  WriteLn(L.IndexOf(15), ' ', L.IndexOf(99), ' ', L.Contains(30), ' ', L.Contains(5));
  L.Add(15);
  WriteLn(L.IndexOf(15), ' ', L.LastIndexOf(15));
  WriteLn(L.Remove(15), ' ', L.Remove(99));
  ShowInts(L);
  L.Exchange(0, L.Count - 1);
  ShowInts(L);
  L.Clear;
  ShowInts(L);
  L.Free;

  S := TList<string>.Create;
  S.Add('apple');
  S.Add('cherry');
  S.Insert(1, 'banana');
  S.Add('');
  ShowStrs(S);
  WriteLn(S.IndexOf('cherry'), ' ', S.IndexOf('Cherry'), ' ', S.IndexOf(''));
  S[0] := S[0] + ' pie';
  S.Delete(1);
  ShowStrs(S);
  WriteLn(S.First, ' ', S.Last = '');
  S.Free;
end.
