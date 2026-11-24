program DumpUnusedClassVars;

// The `class var`s no kept function uses are not global variables of the module (units.md 6.2):
// those of a type never named (TUnused), the unused one of a used type (TUsed.Spare), one only an
// unreachable routine uses (TUsed.Dead) and those of an instantiation whose `class var`s nothing
// uses (TBox<string>).
// The hir keeps every global with its own number; the ir and the bytecode number the kept ones.
// An unused `var` and `ExitCode` stay.

type
  TUnused = class
    class var Hidden: Integer;
    class var Name: string;
  end;

  TUsed = class
    class var Spare: string;
    class var Count: Integer;
    class var Dead: Integer;
    class var Last: string;
  end;

  TBox<T> = class
    class var Made: Integer;
    class var Tag: string;
    constructor Create;
  end;

var
  Unread: Integer;
  B: TBox<Integer>;
  S: TBox<string>;

constructor TBox<T>.Create;
begin
  Inc(Made);
end;

procedure Unreachable;
begin
  TUsed.Dead := 1;
end;

begin
  TUsed.Count := 2;
  TUsed.Last := 'last';
  B := TBox<Integer>.Create;
  TBox<Integer>.Tag := 'int';
  Writeln(TUsed.Count, ' ', TUsed.Last, ' ', TBox<Integer>.Made, ' ', TBox<Integer>.Tag);
  B.Free;
  ExitCode := 0;
end.
