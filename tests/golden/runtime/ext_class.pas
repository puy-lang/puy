// puy-test: test-ext
// An extension class holds a Rust value (`docs/spec/extensions.md` 2.2, 5.4): made by its
// `native` constructor, used through its methods, and dropped with its last reference (the
// values are made in routines, whose temporaries let go of them when they return).
program ExtClass;

uses TestExt;

procedure Use;
var
  A, B: TCounter;
begin
  A := TCounter.Create(10);
  A.Increment;
  A.Increment;
  WriteLn('value ', A.Value, ' live ', TCounter.Live);
  B := A.Same;
  B.Increment;
  WriteLn('same ', A.Value, ' ', A = B, ' live ', TCounter.Live);
  WriteLn(TCounter.Create(5).ClassName, ' live ', TCounter.Live);
end;

procedure Maybe;
begin
  WriteLn(CounterValue(MaybeCounter(True)), ' ', CounterValue(MaybeCounter(False)));
  WriteLn(MaybeCounter(False) = nil, ' live ', TCounter.Live);
end;

function Keep: TCounter;
begin
  Result := TCounter.Create(3);
end;

procedure NilSelf;
var
  C: TCounter = nil;
begin
  try
    C.Increment;
  except
    on E: Exception do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
end;

var
  K: TCounter;
begin
  WriteLn('live ', TCounter.Live);
  Use;
  WriteLn('after Use, live ', TCounter.Live);
  Maybe;
  WriteLn('after Maybe, live ', TCounter.Live);
  K := Keep;
  WriteLn('kept ', K.Value, ' live ', TCounter.Live);
  NilSelf;
end.
