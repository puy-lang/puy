// puy-test: memory-limit=1m
program RttiInvokeLifetime;

// Invoking methods and making attributes through System.Rtti counts what goes in and out like
// the calls written in the code (rtti.md 5.7, 5.8): the arguments, the result, the var
// arguments written back and the method values made for each call are let go of (the loop would
// run out of the 1 MB limit if anything were kept); the attributes' instances live until the
// isolate ends.

uses System.SysUtils, System.Rtti;

type
  TMarkAttribute = class(TCustomAttribute)
  public
    destructor Destroy; override;
  end;

  [TMark]
  TWorker = class
  public
    function Join(const A, B: string): string;
    procedure Append(var S: string; var Xs: TArray<string>);
  end;

destructor TMarkAttribute.Destroy;
begin
  WriteLn('attribute destroyed');
  inherited;
end;

function TWorker.Join(const A, B: string): string;
begin
  Result := A + B;
end;

procedure TWorker.Append(var S: string; var Xs: TArray<string>);
begin
  S := S + '!';
  Xs := Xs + [S];
end;

var
  Ctx: TRttiContext;
  T: TRttiType;
  W: TWorker;
  A: TArray<TValue>;
begin
  Ctx := TRttiContext.Create;
  T := Ctx.GetType(TWorker);
  W := TWorker.Create;
  for var I := 1 to 3000 do
  begin
    var S := T.GetMethod('Join').Invoke(W, [StringOfChar('a', 200), 'b']).AsString;
    if Length(S) <> 201 then
      WriteLn('bad join');
    if Length(T.GetAttributes) <> 1 then
      WriteLn('bad attributes');
    SetLength(A, 2);
    A[0] := StringOfChar('c', 100);
    A[1] := TValue.From<TArray<string>>([StringOfChar('d', 100)]);
    T.GetMethod('Append').Invoke(W, A);
    var Xs := A[1].AsType<TArray<string>>;
    if Length(Xs) <> 2 then
      WriteLn('bad append');
  end;
  var Last := A[1].AsType<TArray<string>>;
  WriteLn(Length(A[0].AsString), ' ', Length(Last[1]));
  A := nil;
  W.Free;
  WriteLn('end');
end.
