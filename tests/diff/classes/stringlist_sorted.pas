program StringListSorted;

// Sorting a TStringList (docs/spec/stdlib/System.Classes/TStringList.md): Sort, Sorted keeping
// the order as strings are added, the three Duplicates, CaseSensitive, Find, CustomSort, and the
// operations a sorted list refuses. The strings are lower-case letters only, or differ in more
// than case, so that every order is the same however equal strings are placed (and with
// Delphi's order of the locale, which puts `b` before `B`).

uses System.SysUtils, System.Classes;

procedure Show(const What: string; L: TStrings);
var
  I: Integer;
begin
  Write(What, ':');
  for I := 0 to L.Count - 1 do
    Write(' ', L[I]);
  WriteLn;
end;

// By length, then as CompareStrings.
function ByLength(List: TStringList; Index1, Index2: Integer): Integer;
begin
  Result := Length(List[Index1]) - Length(List[Index2]);
  if Result = 0 then
    Result := CompareText(List[Index1], List[Index2]);
end;

var
  L: TStringList;
  I: Integer;
begin
  L := TStringList.Create;
  L.CommaText := 'pear,Apple,fig,banana,cherry';
  L.Sort;
  Show('Sort', L);
  L.CustomSort(ByLength);
  Show('CustomSort', L);

  L.Sorted := True;
  Show('Sorted', L);
  WriteLn(L.Add('date'), ' ', L.Add('BANANA'), ' ', L.Count);
  Show('dupIgnore', L);
  WriteLn(L.IndexOf('CHERRY'), ' ', L.IndexOf('grape'));
  I := -1;
  WriteLn(L.Find('date', I), ' ', I);
  WriteLn(L.Find('dog', I), ' ', I);
  WriteLn(L.Find('zzz', I), ' ', I);

  L.Duplicates := TDuplicates.dupAccept;
  WriteLn(L.Add('fig'), ' ', L.Count);
  Show('dupAccept', L);
  L.Duplicates := TDuplicates.dupError;
  try
    L.Add('pear');
  except
    on E: Exception do
      WriteLn(E.ClassName);
  end;
  WriteLn(L.Count);

  try
    L.Insert(0, 'kiwi');
  except
    on E: Exception do
      WriteLn('Insert: ', E.ClassName);
  end;
  try
    L[0] := 'kiwi';
  except
    on E: Exception do
      WriteLn('Strings[]: ', E.ClassName);
  end;
  L.Delete(0);
  Show('Delete', L);

  L.Sorted := False;
  L.Add('aaa');
  Show('unsorted', L);
  L.Sorted := True;
  Show('sorted again', L);

  L.Clear;
  L.Duplicates := TDuplicates.dupIgnore;
  L.CaseSensitive := True;
  L.Add('b');
  L.Add('a');
  L.Add('B');
  L.Add('b');
  WriteLn(L.Count, ' ', L.IndexOf('B') >= 0, ' ', L.IndexOf('A'));
  L.CaseSensitive := False;
  WriteLn(L.IndexOf('A'));
  L.Free;
end.
