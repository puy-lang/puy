program ClassImplicit;

// An instance of a class passed to a record's `Implicit(const A: TBase)` (objects.md 11.4): an
// instance of a descendant goes through the built-in conversion to the ancestor first, in an
// assignment and an argument.

type
  TBase = class
    function Name: string; virtual;
  end;
  TMid = class(TBase)
    function Name: string; override;
  end;
  TLeaf = class(TMid)
    function Name: string; override;
  end;

  THolder = record
    S: string;
    class operator Implicit(const A: TBase): THolder;
  end;

function TBase.Name: string;
begin
  Result := 'base';
end;

function TMid.Name: string;
begin
  Result := 'mid';
end;

function TLeaf.Name: string;
begin
  Result := 'leaf';
end;

class operator THolder.Implicit(const A: TBase): THolder;
begin
  Result := Default(THolder);
  Result.S := 'holder ' + A.Name;
end;

procedure Show(const H: THolder);
begin
  WriteLn(H.S);
end;

var
  M: TMid;
  L: TLeaf;
  H: THolder;
begin
  M := TMid.Create;
  L := TLeaf.Create;
  H := M;
  WriteLn(H.S);
  Show(L);
  M.Free;
  L.Free;
end.
