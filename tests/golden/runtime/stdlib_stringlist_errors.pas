program StringListErrors;

// The exceptions of TStringList (docs/spec/stdlib/System.Classes/TStringList.md 4): an index
// outside the list is ERangeError as for TList<T>, a sorted list refuses to insert, write and
// reorder, dupError, and an owning list refuses an instance of a reference counted class.

uses System.SysUtils, System.Classes;

type
  TCounted = class(TRefObject)
  end;

  TItem = class
  public
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

constructor TItem.Create(const AName: string);
begin
  inherited Create;
  Name := AName;
end;

destructor TItem.Destroy;
begin
  WriteLn('free ', Name);
  inherited Destroy;
end;

procedure Check(const What: string; const P: TProc);
begin
  try
    P();
    WriteLn(What, ': no exception');
  except
    on E: Exception do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message);
  end;
end;

begin
  var L := TStringList.Create;
  L.CommaText := 'a,b,c';
  Check('Strings[3]', procedure begin WriteLn(L[3]) end);
  Check('Strings[-1] :=', procedure begin L[-1] := 'x' end);
  Check('Objects[3]', procedure begin WriteLn(Assigned(L.Objects[3])) end);
  Check('Delete(3)', procedure begin L.Delete(3) end);
  Check('Insert(4)', procedure begin L.Insert(4, 'x') end);
  Check('Exchange(0, 3)', procedure begin L.Exchange(0, 3) end);
  Check('Move(5, 0)', procedure begin L.Move(5, 0) end);
  Check('Names[3]', procedure begin WriteLn(L.Names[3]) end);
  L.Sorted := True;
  Check('Insert', procedure begin L.Insert(0, 'x') end);
  Check('InsertObject', procedure begin L.InsertObject(0, 'x', nil) end);
  Check('Strings[0] :=', procedure begin L[0] := 'x' end);
  Check('Exchange', procedure begin L.Exchange(0, 1) end);
  Check('Move', procedure begin L.Move(0, 1) end);
  Check('Exchange(0, 3) sorted', procedure begin L.Exchange(0, 3) end);
  L.Duplicates := TDuplicates.dupError;
  Check('dupError', procedure begin L.Add('B') end);
  WriteLn(L.CommaText);

  L.Sorted := False;
  L.Objects[0] := TCounted.Create;
  Check('OwnsObjects := True', procedure begin L.OwnsObjects := True end);
  WriteLn(L.OwnsObjects);
  L.Objects[0] := nil;
  L.OwnsObjects := True;
  Check('AddObject', procedure begin L.AddObject('d', TCounted.Create) end);
  Check('InsertObject', procedure begin L.InsertObject(0, 'd', TCounted.Create) end);
  Check('Objects[1] :=', procedure begin L.Objects[1] := TCounted.Create end);
  WriteLn(L.Count);
  // Writing over an object does not free it, even when the list owns its objects
  var Old := TItem.Create('old');
  L.Objects[0] := Old;
  L.Objects[0] := TItem.Create('new');
  Old.Free;
  L.Free;
end.
