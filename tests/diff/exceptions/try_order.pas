program TryOrder;
// The order of execution of try..except and try..finally, including Exit, Break and Continue
// that run finally blocks, and nested finally blocks (docs/spec/exceptions.md 5.2, 5.3).

uses System.SysUtils;

type
  EStep = class(Exception);

function Classify(N: Integer): string;
begin
  Result := 'none';
  try
    WriteLn('  body ', N);
    if N = 0 then
      Exit('zero');
    if N = 1 then
      raise EStep.Create('one');
    Result := 'many';
  finally
    WriteLn('  finally ', N, ' ', Result);
  end;
  WriteLn('  after try ', N);
end;

procedure Nested(N: Integer);
begin
  try
    try
      try
        WriteLn('  level 3');
        if N = 1 then
          Exit;
        if N = 2 then
          raise EStep.Create('deep');
      finally
        WriteLn('  finally 3');
      end;
      WriteLn('  after level 3');
    finally
      WriteLn('  finally 2');
    end;
  finally
    WriteLn('  finally 1');
  end;
  WriteLn('  end of Nested');
end;

procedure Loops;
var
  I, J: Integer;
begin
  for I := 1 to 4 do
  begin
    try
      if I = 2 then
        Continue;
      if I = 4 then
        Break;
      WriteLn('  loop ', I);
    finally
      WriteLn('  loop finally ', I);
    end;
    WriteLn('  loop tail ', I);
  end;
  I := 0;
  while True do
  begin
    Inc(I);
    for J := 1 to 3 do
      try
        try
          if J = 2 then
            Break;
          WriteLn('  inner ', I, ' ', J);
        except
          WriteLn('  never');
        end;
      finally
        WriteLn('  inner finally ', I, ' ', J);
      end;
    try
      if I = 2 then
        Break;
    finally
      WriteLn('  outer finally ', I);
    end;
  end;
  WriteLn('  loops done ', I);
end;

procedure InHandler;
var
  I: Integer;
begin
  for I := 1 to 3 do
    try
      try
        raise EStep.Create('h');
      except
        on E: EStep do
        begin
          WriteLn('  handler ', I);
          if I = 1 then
            Continue;
          if I = 2 then
            Exit;
        end;
      end;
      WriteLn('  never after handler');
    finally
      WriteLn('  handler finally ', I);
    end;
end;

begin
  WriteLn('Classify');
  WriteLn(Classify(0));
  WriteLn(Classify(2));
  try
    WriteLn(Classify(1));
  except
    on E: EStep do
      WriteLn('caught ', E.Message);
  end;
  WriteLn('Nested');
  Nested(0);
  Nested(1);
  try
    Nested(2);
  except
    WriteLn('caught deep');
  end;
  WriteLn('Loops');
  Loops;
  WriteLn('InHandler');
  InHandler;
  WriteLn('done');
end.
