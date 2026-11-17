program ProcTypeErrors;

// Errors in the declaration of procedural types (objects.md 9.1, 9.10): a default parameter
// value (E0752), `reference to … of object` (E0400 at `of`), a procedural type where only a type
// name can be written (E0404: a parameter, a return type, a `for var` variable and the argument
// of `TArray<…>`), a calling convention (W0009, removed by its fix), and a class implementing an
// anonymous method type (E3200).

type
  TDefault = procedure(X: Integer = 1);
  TObjRef = reference to procedure of object;
  TConv = procedure(X: Integer); stdcall;
  TRun = reference to procedure;
  TRunner = class(TRefObject, TRun)
  end;
  TList = TArray<procedure>;

procedure Take(P: procedure);
begin
end;

function Give: function: Integer;
begin
end;

var
  Nums: TArray<Integer>;
begin
  for var P: procedure in Nums do
    WriteLn;
end.
