program PropertyAccessorErrors;

// Variants of the property diagnostics not covered by golden/diag/property_errors (objects.md
// 2.6): a class function as the accessor of an instance property, an accessor the class cannot
// see (E0743), a `write` method with a `var` parameter or a parameter of another type, the notes
// for an `index N` property's `write`, `index` that is not an integer, not a constant or out of
// range, and a property passed to `ReadLn` and `SetLength`.

var
  GIndex: Integer;

type
  TBase = class
  strict private
    FHidden: Integer;
  end;

  TFoo = class(TBase)
    FS: string;
    class function GetCount: Integer;
    procedure SetVar(var V: Integer);
    procedure SetWide(V: Int64);
    function GetAt(Index: Integer): Integer;
    property Count: Integer read GetCount;
    property Hidden: Integer read FHidden;
    property A: Integer write SetVar;
    property B: Integer write SetWide;
    property C: Integer index 1 write SetWide;
    property D: Integer index 'ab' read GetAt;
    property E: Integer index GIndex read GetAt;
    property F: Integer index 3000000000 read GetAt;
    property S: string read FS write FS;
  end;

class function TFoo.GetCount: Integer;
begin
  Result := 0;
end;

procedure TFoo.SetVar(var V: Integer);
begin
end;

procedure TFoo.SetWide(V: Int64);
begin
end;

function TFoo.GetAt(Index: Integer): Integer;
begin
  Result := Index;
end;

var
  O: TFoo;
begin
  GIndex := 1;
  O := TFoo.Create;
  ReadLn(O.S);
  SetLength(O.S, 3);
end.
