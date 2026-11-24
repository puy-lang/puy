program StringListOrder;

// The order of TStringList (docs/spec/stdlib/System.Classes/TStringList.md 3.2): code point
// order when CaseSensitive, Unicode lower case folding when not, a stable sort that keeps equal
// strings in the order they were, a sorted list with dupAccept inserting before equal strings,
// and a CustomSort that changes the list (still telling OnChange). Also the slices and the
// arrays of the list.

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

type
  TWatch = class
  public
    procedure Changing(Sender: TObject);
    procedure Changed(Sender: TObject);
  end;

procedure TWatch.Changing(Sender: TObject);
begin
  Write('<');
end;

procedure TWatch.Changed(Sender: TObject);
begin
  Write('>');
end;

function Shrinking(List: TStringList; Index1, Index2: Integer): Integer;
begin
  if List.Count > 2 then
    List.Delete(List.Count - 1);
  Result := 0;
end;

begin
  var L := TStringList.Create;
  L.CommaText := 'b,B,a,_,A,Ä,ä,z,Z,ab';
  L.CaseSensitive := True;
  L.Sort;
  Show('case sensitive', L);
  L.CaseSensitive := False;
  L.CommaText := 'b,B,a,_,A,Ä,ä,z,Z,ab';
  L.Sort;
  Show('not case sensitive', L);
  WriteLn(L.IndexOf('Ä'), ' ', L.IndexOf('ÄB'));

  L.Clear;
  L.Sorted := True;
  L.Duplicates := TDuplicates.dupAccept;
  L.Add('x');
  L.Add('X');
  L.Add('a');
  L.Add('x');
  for var I := 0 to L.Count - 1 do
    Write(L[I], ' ');
  WriteLn;
  var I := -1;
  WriteLn(L.Find('X', I), ' ', I);
  L.CaseSensitive := True;
  Show('resorted', L);

  L.Sorted := False;
  L.CommaText := 'c,b,a';
  // OnChange follows OnChanging even when the sort raises
  var W := TWatch.Create;
  L.OnChanging := W.Changing;
  L.OnChange := W.Changed;
  try
    L.CustomSort(Shrinking);
  except
    on E: EInvalidOperation do
      WriteLn(' ', E.Message);
  end;
  L.OnChanging := nil;
  L.OnChange := nil;
  W.Free;
  Show('after', L);

  L.CommaText := 'p,q,r,s,t';
  var A := L[1..3];
  WriteLn(Length(A), ' ', A[0], ' ', A[2]);
  WriteLn(Length(L[3..]), ' ', Length(L[..1]), ' ', Length(L[7..9]));
  WriteLn(string.Join('+', L.ToStringArray), ' ', Length(L.ToObjectArray));
  L.Append('u');
  L.AddStrings(['v', 'w']);
  Show('Append, AddStrings', L);
  L.Capacity := 100;
  WriteLn(L.Capacity >= 100, ' ', L.Count);
  WriteLn(L.Encoding = nil);
  L.Free;
end.
