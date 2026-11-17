program DumpCtorDtor;

// The dumps of constructors and destructors (objects.md 14.3, 14.8, 15): `create TFoo fN` with
// the arguments, `inherited builtin` for `TObject.Create` and `TObject.Destroy`, and the
// `constructor` / `destructor` methods (hir); the creation sequence (the arguments, `New`, the
// `try` around `CallMethod` and the `AfterConstruction` `CallVirtual`, and `DestroyFailed` in the
// `finally` block) and `DtorEnter` at the start of a destructor (ir, bytecode).

type
  TNamed = class
  public
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
    procedure AfterConstruction; override;
  end;

constructor TNamed.Create(const AName: string);
begin
  inherited Create;
  Name := AName;
end;

destructor TNamed.Destroy;
begin
  WriteLn('bye ', Name);
  inherited Destroy;
end;

procedure TNamed.AfterConstruction;
begin
  WriteLn('hello ', Name);
end;

var
  N: TNamed;
begin
  N := TNamed.Create('dump');
  N.Free;
end.
