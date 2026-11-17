program ProcTypeSyntax;

// The syntax of procedural types (objects.md 9.1, grammar.md 2.2): `PROC_TYPE` with the
// parameters, the return type, `of object` and a calling convention after `;`, and `REF_TYPE`
// around a `PROC_TYPE`, in a `type` section, a variable and a field.

type
  TFunc = function(A, B: Integer): Integer;
  TEvent = procedure(Sender: TObject) of object;
  TGetter = function: string of object;
  TCall = procedure; cdecl;
  TRun = reference to procedure(const S: string);
  TNext = reference to function: Integer;
  THolder = record
    OnDone: procedure;
  end;

var
  P: procedure(X: Integer);

begin
end.
