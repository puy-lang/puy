program StringListObjects;

// The objects of a TStringList (docs/spec/stdlib/System.Classes/TStringList.md): AddObject,
// InsertObject, Objects[], IndexOfObject, the objects moving with their strings when sorted,
// OwnsObjects freeing them on Delete, Clear and Free, and OnChanging / OnChange with
// BeginUpdate / EndUpdate. A destructor writes a line, so the output shows which are freed.

uses System.SysUtils, System.Classes;

type
  TItem = class
  public
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  TWatcher = class
  public
    Changes: Integer;
    procedure Changing(Sender: TObject);
    procedure Changed(Sender: TObject);
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

procedure TWatcher.Changing(Sender: TObject);
begin
  Write('<');
end;

procedure TWatcher.Changed(Sender: TObject);
begin
  Inc(Changes);
  Write('>');
end;

procedure Show(const What: string; L: TStrings);
var
  I: Integer;
begin
  Write(What, ':');
  for I := 0 to L.Count - 1 do
    if L.Objects[I] = nil then
      Write(' ', L[I], '/nil')
    else
      Write(' ', L[I], '/', TItem(L.Objects[I]).Name);
  WriteLn;
end;

var
  L: TStringList;
  A, B, C: TItem;
  W: TWatcher;
begin
  A := TItem.Create('A');
  B := TItem.Create('B');
  C := TItem.Create('C');
  L := TStringList.Create;
  L.AddObject('c', C);
  L.AddObject('a', A);
  L.InsertObject(1, 'b', B);
  L.Add('none');
  Show('added', L);
  WriteLn(L.IndexOfObject(B), ' ', L.IndexOfObject(nil), ' ', L.IndexOf('none'));
  L.Sort;
  Show('sorted', L);
  L.Objects[3] := C;
  L[0] := 'first';
  Show('written', L);
  L.Free;
  WriteLn('not owned: nothing freed');

  L := TStringList.Create;
  L.OwnsObjects := True;
  L.AddObject('a', A);
  L.AddObject('b', B);
  L.AddObject('c', C);
  L.AddObject('d', TItem.Create('D'));
  L.AddObject('e', TItem.Create('E'));
  L.Add('none');
  WriteLn(L.OwnsObjects);
  L.Delete(1);
  L.Delete(L.Count - 1);
  Show('owned', L);
  L.Exchange(0, 1);
  L.Move(0, 2);
  Show('reordered', L);
  L.Clear;
  WriteLn('cleared ', L.Count);
  L.AddObject('f', TItem.Create('F'));
  L.AddObject('g', TItem.Create('G'));
  L.Free;
  WriteLn('freed');

  W := TWatcher.Create;
  L := TStringList.Create;
  L.OnChanging := W.Changing;
  L.OnChange := W.Changed;
  L.Add('x');
  L.Add('y');
  L[0] := 'z';
  L.Delete(1);
  WriteLn(' ', W.Changes);
  L.BeginUpdate;
  L.Add('p');
  L.BeginUpdate;
  L.Add('q');
  L.EndUpdate;
  L.Add('r');
  L.EndUpdate;
  WriteLn(' ', W.Changes);
  L.Text := 'one'#10'two'#10'three';
  WriteLn(' ', W.Changes, ' ', L.Count);
  L.Sort;
  L.Clear;
  WriteLn(' ', W.Changes);
  L.Clear;
  WriteLn(' ', W.Changes);
  L.Free;
  W.Free;
end.
